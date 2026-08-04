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
# FRESHNESS IS A SEPARATE AXIS FROM ARGV, and the argv replay above cannot reach it: its
# oracle is a hardcoded prebuilt path and nothing here ever runs `cargo build`, so its
# verdicts are accepted / rejected / no-binary — it reports a MISSING binary, never a STALE
# one. A pre-merge binary therefore made the chain and its own argv guard agree on stale
# behaviour, both reading the same artifact and neither able to see that it was old. The
# preflight freshness section below covers that axis against STUB binaries whose mtimes and
# contents the fixture controls outright: cargo's dep-info shape is reproduced as one-line
# `.d` files, staleness is manufactured with `touch -t`, and every registered probe literal
# is read FROM THE SCRIPT and planted in its stub so the fixture cannot drift from it.
#
# SCOPE LIMITS.
#   * Step [10] (lab-mount-universe) and the step [11] GO/NO-GO report are still never
#     reached — the catch-up runs stop one step short of them by design, and nothing here
#     drives the attended path past the 09:00 guard.
#   * The freshness axes are exercised against STUBS, so they prove the SCRIPT's logic, not
#     that any real binary is current. Nothing here builds or replays `calendar-refresh` —
#     the binary that actually carries PR #258's guard remains the structurally least
#     covered one, which is a real gap on a different axis.
#   * `make script-check` is not a `make gate-run` step and no CI workflow invokes it, so the
#     R10 literal-drift assertion below makes a reworded probe literal DIAGNOSABLE, not
#     preempted: a reword still reaches the 08:45 chain as a hard exit 64.

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

# The seven binaries the preflight requires, named ONCE. make_fixture needs this list three times
# over (a source file, a dep-info file, and the stub itself), and three hand-kept copies would drift
# exactly the way session-morning.sh's own comment warns about for the real preflight loop.
FIXTURE_STUB_BINS="calendar-fetch-inputs calendar-refresh calendar-activate calendar-status
ls-ingest lab-research lab-mount-universe"

# The probe-literal registry, read FROM THE SCRIPT UNDER TEST rather than restated here. Two
# things need it — the fixture, which must plant every registered literal in the stub it is
# registered for, and the drift assertion (R10) — and a second copy in this file would be exactly
# the hand-kept mirror that the argv replay above exists to avoid.
registry_entries() { # -> one "<binary>|<literal>|<provenance>" line per registry entry
  sed -n '/^BIN_PROBE_LITERALS=(/,/^)$/p' "$REAL_SCRIPT" | sed -n 's/^  "\(.*\)"$/\1/p'
}

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

  # ---- sources for the freshness axes, written BEFORE the stubs so the stubs are newer -------
  # The preflight compares each binary against the source set cargo recorded in $BIN/<name>.d,
  # so the fixture needs sources to compare against. Their paths deliberately MIRROR the real
  # layout across BOTH workspaces: an adapter-side src/bin file, a ROOT-CRATE file, and a
  # repo-root metadata/ file — the build-script input that no src/ scan would ever reach and
  # that a metadata/constraints/*.yaml edit moves without touching any src/ directory. That is
  # what lets the cross-workspace reach be exercised here rather than only on the operator's tree.
  #
  # Write order is load-bearing: sources first, stubs second, so every stub is at least as new as
  # every source and the DEFAULT fixture is fresh. Staleness is then manufactured explicitly by
  # the touch -t knobs at the end of this function, never by accident of ordering.
  local src_bin="$naut/src/bin" src_core="$root/crates/ls-core/src" src_meta="$root/metadata/constraints"
  mkdir -p "$src_bin" "$src_core" "$src_meta" "$naut/lab" "$naut/nautilus-ls-calendar"
  printf '%s\n' 'fn main() {}' >"$src_core/lib.rs"
  printf '%s\n' 'fixture: true' >"$src_meta/fixture.yaml"
  # The MANIFESTS cargo's dep-info records nowhere, which the preflight folds in by hand because a
  # manifest-only change (dep bump, `cargo update`, feature flip, toolchain pin) dirties every
  # binary while leaving each recorded source older than it. The fixture must carry all of them:
  # they count toward `vanished` when absent, so a fixture missing one would mark every stub stale.
  local m
  for m in "$root/Cargo.toml" "$root/Cargo.lock" "$naut/Cargo.toml" "$naut/Cargo.lock" \
           "$naut/rust-toolchain.toml" "$naut/lab/Cargo.toml" "$naut/nautilus-ls-calendar/Cargo.toml"; do
    printf '%s\n' '# fixture manifest' >"$m"
  done
  for sb in $FIXTURE_STUB_BINS; do
    printf '%s\n' 'fn main() {}' >"$src_bin/$sb.rs"
  done

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
  # The checkpoint's daily watermarks are per-test knobs: the window-derivation tests
  # below set FIXTURE_WM_A behind FIXTURE_WM_B to prove the MIN governs, and FIXTURE_CKPT
  # replaces the whole document to exercise the empty-watermark refusal. The defaults
  # reproduce the designed one-session-per-morning cadence (frontier = the prior session).
  if [ -n "${FIXTURE_CKPT:-}" ]; then
    printf '%s\n' "$FIXTURE_CKPT" >"$root/data/turn4-fresh/catalog/ingest-checkpoint.json"
  else
    printf '%s\n' "{\"watermarks\":{\"005930.XKRX|1-DAY\":\"${FIXTURE_WM_A:-20260729}\",\"000660.XKRX|1-DAY\":\"${FIXTURE_WM_B:-20260729}\"},\"gaps\":[],\"shifted\":{}}" \
      >"$root/data/turn4-fresh/catalog/ingest-checkpoint.json"
  fi

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

  # ---- dep-info beside every stub, in cargo's make-rule shape --------------------------------
  # "<target>: <src> <src> ..." with absolute paths, which is exactly what the preflight parses.
  # FIXTURE_DROP_DEPINFO_FOR withholds one on purpose: that is how the "freshness UNEVALUABLE"
  # arm is reached, and it must refuse rather than fall through to pass.
  for sb in $FIXTURE_STUB_BINS; do
    [ "$sb" = "${FIXTURE_DROP_DEPINFO_FOR:-}" ] && continue
    printf '%s: %s %s %s\n' \
      "$bin/$sb" "$src_bin/$sb.rs" "$src_core/lib.rs" "$src_meta/fixture.yaml" >"$bin/$sb.d"
  done

  # ---- every REGISTERED probe literal, planted in the stub it is registered for ---------------
  # Without this the content axis refuses every fixture chain: the calendar-refresh stub is a bash
  # heredoc containing no such string, so ~30 existing assertions would fail before reaching what
  # they actually test. A comment line satisfies `grep -qaF`. The registry is read from the script
  # (registry_entries) rather than restated, so the fixture cannot drift from what it must satisfy.
  # FIXTURE_OMIT_LITERAL_FOR withholds one deliberately — that, never the default stub, is how the
  # literal-absent refusal is reached.
  local rb rlit
  while IFS='|' read -r rb rlit _; do
    [ -n "$rb" ] || continue
    [ "$rb" = "${FIXTURE_OMIT_LITERAL_FOR:-}" ] && continue
    [ -f "$bin/$rb" ] || continue
    printf '# probe literal (fixture stub): %s\n' "$rlit" >>"$bin/$rb"
  done < <(registry_entries)

  # FIXTURE_DELETE_SRC_FOR removes a source that the binary's .d still LISTS, and touches no mtime
  # at all. That isolates the `vanished` half of the staleness test: every surviving source stays
  # older than the binary, so the mtime comparison cannot fire and only the vanished-source count
  # can refuse. Cargo treats a target whose recorded source is gone as dirty, so the binary really
  # is stale even though nothing it still has is newer than it.
  if [ -n "${FIXTURE_DELETE_SRC_FOR:-}" ]; then
    rm -f "$src_bin/$FIXTURE_DELETE_SRC_FOR.rs"
  fi

  # FIXTURE_DROP_BIN removes a stub outright (with its dep-info), reaching the ABSENT arm.
  # FIXTURE_UNEXEC_BIN strips the execute bit, which the binary class treats as absent too — the
  # `-e` to `-x` tightening, unreachable while make_fixture chmod +x's every stub.
  if [ -n "${FIXTURE_DROP_BIN:-}" ]; then
    rm -f "$bin/$FIXTURE_DROP_BIN" "$bin/$FIXTURE_DROP_BIN.d"
  fi
  if [ -n "${FIXTURE_UNEXEC_BIN:-}" ]; then
    chmod -x "$bin/$FIXTURE_UNEXEC_BIN"
  fi

  # ---- staleness knobs. touch -t is portable across macOS and GNU ----------------------------
  #   FIXTURE_AGE_BIN=<name>    age ONE stub behind its own inputs, so that binary ALONE is stale
  #                             — the per-binary property a single shared timestamp cannot express
  #   FIXTURE_STALE_VIA=<rel>   age every stub AND every input, then return ONE input to "now",
  #                             making that input the sole reason the binaries are stale
  if [ -n "${FIXTURE_AGE_BIN:-}" ]; then
    touch -t 202601010000 "$bin/$FIXTURE_AGE_BIN"
  fi
  if [ -n "${FIXTURE_STALE_VIA:-}" ]; then
    find "$bin" -type f -exec touch -t 202601010000 {} +
    # The MANIFESTS must be aged with the sources. They are part of the compared set, so leaving
    # them at "now" would make every stub stale for a reason the test did not choose, and this
    # knob's whole purpose is that ONE named input is the sole cause.
    find "$src_bin" "$src_core" "$src_meta" -type f -exec touch -t 202512310000 {} +
    touch -t 202512310000 "$root/Cargo.toml" "$root/Cargo.lock" "$naut/Cargo.toml" \
      "$naut/Cargo.lock" "$naut/rust-toolchain.toml" "$naut/lab/Cargo.toml" \
      "$naut/nautilus-ls-calendar/Cargo.toml"
    touch "$root/$FIXTURE_STALE_VIA"
  fi

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

drop_fixture() {
  [ -n "${CHAIN_ROOT:-}" ] && rm -rf "$CHAIN_ROOT"
  CHAIN_ENV=(); FIXTURE_WM_A=""; FIXTURE_WM_B=""; FIXTURE_CKPT=""
  FIXTURE_AGE_BIN=""; FIXTURE_STALE_VIA=""; FIXTURE_DROP_DEPINFO_FOR=""; FIXTURE_OMIT_LITERAL_FOR=""
  FIXTURE_DELETE_SRC_FOR=""; FIXTURE_DROP_BIN=""; FIXTURE_UNEXEC_BIN=""
  return 0
}

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

# ===================================================================== the gap window
# THE THIRD DEFECT CLASS: step [3]'s window seeded from the SESSION DATE under-fetches after
# a MISSED morning. window.from is the START of the KRX witness fetch, and the ingest's
# accumulate plan acts only on the established prefix — it stops before the first
# calendar-Unknown day and never crosses it (established_prefix in src/ingest/mod.rs) — so a
# 2+ session gap got a witness only for the LATEST day and the bounded ingest stalled at the
# frontier with nothing advanced. The fix derives window.from = min(daily watermark) + 1,
# clamped to the session date. Invisible on 2026-07-31 only because that gap was one day.
#
# The fixture watermarks are deliberately MIXED (20260725 behind the 20260729 default): the
# MIN must govern, because each symbol's plan starts at ITS OWN watermark+1 and the slowest
# symbol decides how far back the witness fetch has to reach.
FIXTURE_WM_A=20260725
run_chain --stop-before-activate
case "$CHAIN_LOG" in
  *"--window 2026-07-26..$SESSION_DATE"*)
    ok "step [3] window reaches back to the daily frontier + 1 on a multi-session gap" ;;
  *)
    no "step [3] window reaches back to the daily frontier + 1 on a multi-session gap" \
       "--window 2026-07-26..$SESSION_DATE (min watermark 20260725 + 1)" "$CHAIN_LOG" ;;
esac
# The fetch checkpoint must be KEYED ON BOTH WINDOW ENDS: calendar-fetch-inputs refuses to
# resume a checkpoint whose (from, through, krx_through) triple differs (CheckpointMismatch),
# and both ends move within a day — a completed ingest advances the frontier (later
# window.from on the documented recovery re-run), and a run that dies mid-fetch followed by
# a same-day catch-up targeting the NEXT session changes the end while the start stays put.
# A key missing either end hands one of those re-runs the stale checkpoint and dies at [3].
case "$CHAIN_LOG" in
  *"--state "*"-from20260726-to$SESSION_COMPACT"*)
    ok "step [3] fetch checkpoint is keyed on both ends of the derived window" ;;
  *)
    no "step [3] fetch checkpoint is keyed on both ends of the derived window" \
       "a --state path containing -from20260726-to$SESSION_COMPACT" "$CHAIN_LOG" ;;
esac
# And the real parser + confine() must accept the gap-window argv exactly as they accept the
# single-day one — a multi-day window that only a stub ever parsed would be the same silent
# drift this file exists to kill.
FETCH_ARGV="$(fetch_argv_from_log "$CHAIN_LOG")"
if [ -z "$FETCH_ARGV" ]; then
  no "gap-window argv is replayable against the real binary" "a logged argv" "$CHAIN_LOG"
else
  VERDICT="$(replay_real_binary "$FETCH_ARGV")"
  case "$VERDICT" in
    accepted) ok "real calendar-fetch-inputs accepts the gap-window argv from a foreign CWD" ;;
    nobinary) no "real calendar-fetch-inputs accepts the gap-window argv from a foreign CWD" \
                 "a compiled $REAL_BIN (run: cargo build --bin calendar-fetch-inputs)" "binary not built" ;;
    *)        no "real calendar-fetch-inputs accepts the gap-window argv from a foreign CWD" \
                 "argument parsing + state-root confinement to pass" "$VERDICT" ;;
  esac
fi
drop_fixture

# NEGATIVE META-TEST: prove the gap assertion can SEE the pre-fix window. Reverting the
# invocation to the session-date window (the exact pre-fix argv) on the SAME gap fixture
# must produce the under-fetching single-day window — if the two runs were not
# distinguishable here, the positive assertion above would be unfalsifiable theatre.
FIXTURE_WM_A=20260725
run_chain_mutated 's/--window "\$window_from/--window "\$session_date/' --stop-before-activate
case "$CHAIN_LOG" in
  *"--window $SESSION_DATE..$SESSION_DATE"*)
    ok "harness detects a step [3] reverted to the session-date window" ;;
  *)
    no "harness detects a step [3] reverted to the session-date window" \
       "the mutated argv to show --window $SESSION_DATE..$SESSION_DATE" "$CHAIN_LOG" ;;
esac
drop_fixture

# THE CLAMP'S ACTIVE BRANCH. Every fixture above keeps the frontier AT OR BEHIND the prior
# session, so frontier+1 never exceeds the session date and min() is inert — a derivation
# stripped of the clamp would pass every test so far, and the clamp is the only thing
# standing between an already-caught-up catalog and an INVERTED window (from > through).
# Watermarks at and past the session date (a catalog that already ingested it, e.g. after
# a completed catch-up earlier the same day) must clamp back to the session date exactly.
FIXTURE_WM_A=20260730
FIXTURE_WM_B=20260731
run_chain --stop-before-activate
case "$CHAIN_LOG" in
  *"--window $SESSION_DATE..$SESSION_DATE"*)
    ok "step [3] window clamps to the session date when the frontier is at or past it" ;;
  *)
    no "step [3] window clamps to the session date when the frontier is at or past it" \
       "--window $SESSION_DATE..$SESSION_DATE (min watermark 20260730 + 1, clamped)" "$CHAIN_LOG" ;;
esac
drop_fixture

# An unreadable or 1-DAY-empty checkpoint leaves no honest window.from. The run must die
# loudly at the derivation, BEFORE any fetch — a guessed or defaulted window would present
# a partial acquisition as the designed single-day cadence.
FIXTURE_CKPT='{"watermarks":{},"gaps":[],"shifted":{}}'
run_chain --stop-before-activate
assert_eq "an empty daily-watermark set refuses the window derivation (NO-GO 1)" "1" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"could not derive the fetch window start"*)
    ok "the derivation refusal names itself" ;;
  *)
    no "the derivation refusal names itself" \
       "a 'could not derive the fetch window start' message" "$CHAIN_OUT" ;;
esac
case "$CHAIN_LOG" in
  *"calendar-fetch-inputs"*)
    no "no fetch is issued after a refused derivation" "no calendar-fetch-inputs call" "$CHAIN_LOG" ;;
  *) ok "no fetch is issued after a refused derivation" ;;
esac
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

# ============================================================ the preflight freshness axes
# THE FOURTH DEFECT CLASS: the preflight validated all twelve required paths with an EXISTENCE
# test, so a compiled binary older than the sources it was built from reported `ok`. On 2026-08-04
# the tree was clean at 92ba1ed while target/debug/calendar-refresh was built 19 minutes BEFORE
# src/bin/calendar-refresh.rs, and all twelve lines read `ok`. Had the run continued it would have
# executed a calendar-refresh predating PR #258's forward-horizon guard — and that guard's whole
# purpose is to make a refusal observable, so the missing line would have read as a clean pass.
#
# --dry-run IS THE VEHICLE, and the only one available: the --self-test block exits BEFORE
# preflight is reached, while the --dry-run block sits AFTER it. So these runs reach every added
# check and still issue zero traffic.
#
# WHY MUTATION IS NOT OPTIONAL HERE. Per
# docs/solutions/conventions/coverage-only-change-is-verified-by-mutation-not-by-the-gate.md this
# is a regression guard for an already-fixed bug — the 2026-08-04 binaries have since been rebuilt,
# so it passes on arrival and a green gate proves nothing about whether it would have caught the
# original. The two negative meta-tests at the end of this section are the actual proof.

# --- fresh passes, and the registry is sparse ------------------------------------------------
run_chain --dry-run
assert_eq "fresh fixture binaries pass both freshness axes" "0" "$CHAIN_RC"
drop_fixture

# The registry is SPARSE by design: a binary with no entry passes the content axis regardless of
# its contents. The six unregistered stubs in the run above are exactly that case — none contains
# any registered literal and none was refused. Pin the sparseness so that if someone ever registers
# all seven, this claim is re-examined rather than silently voided.
REG_COUNT="$(registry_entries | grep -c . | tr -d ' ')"
if [ "$REG_COUNT" -ge 1 ] && [ "$REG_COUNT" -lt 7 ]; then
  ok "the probe-literal registry is sparse ($REG_COUNT of 7 binaries registered)"
else
  no "the probe-literal registry is sparse" "between 1 and 6 entries" "$REG_COUNT"
fi

# --- stale by mtime, and ONLY the aged binary is implicated -----------------------------------
FIXTURE_AGE_BIN=calendar-refresh
run_chain --dry-run
assert_eq "a binary older than its own sources refuses the preflight (64)" "64" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"are STALE: calendar-refresh"*) ok "the stale refusal names the failing binary" ;;
  *) no "the stale refusal names the failing binary" "'are STALE: calendar-refresh'" "$CHAIN_OUT" ;;
esac
# THE SHARED-TIMESTAMP REGRESSION THIS DESIGN EXISTS TO AVOID. Comparing all seven against one
# newest-source value is unrecoverable: cargo relinks only DIRTY targets, so rebuilding the touched
# binary leaves the other six behind the new shared value with cargo declining to rebuild them.
# The reported COUNT is what proves each binary is compared against its own dep-info set.
case "$CHAIN_OUT" in
  *"error: 1 required binary(ies) are STALE:"*)
    ok "only the aged binary is implicated — each binary has its OWN source set" ;;
  *) no "only the aged binary is implicated — each binary has its OWN source set" \
        "exactly 1 stale binary reported" "$CHAIN_OUT" ;;
esac
case "$CHAIN_OUT" in
  *"cargo build --workspace --bin calendar-refresh"*)
    ok "the stale refusal names the exact rebuild command" ;;
  *) no "the stale refusal names the exact rebuild command" \
        "a 'cargo build --workspace --bin calendar-refresh' line" "$CHAIN_OUT" ;;
esac
# A bare `cargo build` at the REPO ROOT resolves against the other workspace and cannot produce
# these binaries at all, so naming the workspace is part of the remedy, not decoration.
case "$CHAIN_OUT" in
  *"from the adapters/nautilus workspace"*)
    ok "the stale refusal names the workspace the rebuild runs from" ;;
  *) no "the stale refusal names the workspace the rebuild runs from" \
        "the adapters/nautilus workspace named" "$CHAIN_OUT" ;;
esac
case "$CHAIN_LOG" in
  *calendar-fetch-inputs*|*ls-ingest*)
    no "a freshness refusal issues no traffic" "no binary invoked" "$CHAIN_LOG" ;;
  *) ok "a freshness refusal issues no traffic" ;;
esac
drop_fixture

# --- the OTHER half of the staleness test, isolated: a source that no longer exists -------------
# `bin_mtime < src_mtime || vanished > 0` has two independent halves, and every case above exercises
# only the first. Here a source the .d still lists is DELETED while every surviving source stays
# older than the binary, so the mtime comparison cannot fire — only the vanished count can refuse.
# Without this the `|| vanished > 0` clause would have no coverage of its own, and a regression
# dropping it would pass every other assertion in this file.
FIXTURE_DELETE_SRC_FOR=calendar-refresh
run_chain --dry-run
assert_eq "a binary built from a source that no longer exists refuses (64)" "64" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"are STALE: calendar-refresh"*) ok "the vanished-source case is reported as staleness" ;;
  *) no "the vanished-source case is reported as staleness" \
        "'are STALE: calendar-refresh'" "$CHAIN_OUT" ;;
esac
drop_fixture

# --- the ABSENT arm, and the -e -> -x tightening ----------------------------------------------
# Both were previously reachable only on a real tree. The Makefile's script-check comment claims all
# four refusal causes are covered, so leaving these two untested made that claim false.
FIXTURE_DROP_BIN=calendar-refresh
run_chain --dry-run
assert_eq "an absent required binary refuses the preflight (64)" "64" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"ABSENT BINARY"*) ok "the absent refusal is distinct from the stale and unevaluable messages" ;;
  *) no "the absent refusal is distinct from the stale and unevaluable messages" \
        "an 'ABSENT BINARY' message" "$CHAIN_OUT" ;;
esac
case "$CHAIN_OUT" in
  *"cargo build --workspace"*" --bin calendar-refresh"*)
    ok "the absent refusal names the rebuild command too" ;;
  *) no "the absent refusal names the rebuild command too" \
        "a cargo build line naming calendar-refresh" "$CHAIN_OUT" ;;
esac
drop_fixture

# A present-but-unexecutable artifact is as unusable as an absent one, so the binary class tests -x.
FIXTURE_UNEXEC_BIN=calendar-refresh
run_chain --dry-run
assert_eq "a present but NON-EXECUTABLE binary is refused, not reported ok (64)" "64" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"MISS "*calendar-refresh*) ok "the non-executable binary is reported MISS rather than ok" ;;
  *) no "the non-executable binary is reported MISS rather than ok" \
        "a MISS verdict line for calendar-refresh" "$CHAIN_OUT" ;;
esac
drop_fixture

# --- the MANIFESTS, which cargo's dep-info records nowhere -------------------------------------
# `calendar-refresh.d` contains zero Cargo.toml / Cargo.lock / toolchain entries, so a
# manifest-only change (dep bump, `cargo update`, feature flip, toolchain pin) dirties every binary
# per cargo while leaving every recorded source older than the artifact. Dep-info alone therefore
# reported `ok` for seven binaries built from superseded dependency code, and the content axis
# cannot help — a manifest change removes no registered literal. Each manifest gets its own case
# because they are a hand-listed set: a typo in one would otherwise be invisible.
for MANIFEST in Cargo.toml Cargo.lock adapters/nautilus/Cargo.toml adapters/nautilus/Cargo.lock \
                adapters/nautilus/rust-toolchain.toml adapters/nautilus/lab/Cargo.toml \
                adapters/nautilus/nautilus-ls-calendar/Cargo.toml; do
  FIXTURE_STALE_VIA="$MANIFEST"
  run_chain --dry-run
  assert_eq "a binary older than $MANIFEST refuses (64) — dep-info lists no manifest" \
            "64" "$CHAIN_RC"
  drop_fixture
done

# --- the CROSS-WORKSPACE reach: a root-crate source, which no adapter-only scan would see ------
FIXTURE_STALE_VIA=crates/ls-core/src/lib.rs
run_chain --dry-run
assert_eq "a binary older than a ROOT-CRATE source refuses (64) — the cross-workspace axis" \
          "64" "$CHAIN_RC"
drop_fixture

# --- and the BUILD-SCRIPT inputs: crates/ls-core/build.rs embeds the repo-root metadata/ tree at
# compile time, so a metadata/constraints/*.yaml edit changes every binary's behaviour while moving
# no file under any src/ directory. That is the false-green class this axis exists to close, and it
# is only reachable because the source set comes from cargo's dep-info rather than a src/ scan.
FIXTURE_STALE_VIA=metadata/constraints/fixture.yaml
run_chain --dry-run
assert_eq "a binary older than a repo-root metadata/ input refuses (64)" "64" "$CHAIN_RC"
drop_fixture

# --- freshness UNEVALUABLE must refuse, never fall through to pass ----------------------------
# The script runs under `set -uo pipefail` with NO `-e`, so a failed scan neither aborts nor
# refuses on its own — an absent answer reading as "fresh" is precisely the false green this
# closes. count_advanced's -1 sentinel is the shape being copied.
FIXTURE_DROP_DEPINFO_FOR=calendar-status
run_chain --dry-run
assert_eq "a binary with no dep-info file refuses (64) rather than passing" "64" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"UNEVALUABLE for 1 required binary(ies): calendar-status"*)
    ok "the unevaluable refusal names itself and the binary" ;;
  *) no "the unevaluable refusal names itself and the binary" \
        "an 'UNEVALUABLE for 1 required binary(ies): calendar-status' message" "$CHAIN_OUT" ;;
esac
# THREE DISTINCT MESSAGES, not one "rebuild and re-run" line. Per
# docs/solutions/workflow-issues/shell-script-live-path-needs-stubbed-binary-tests.md a handler
# must discriminate among all the ways it can fire, not assert the one cause its author had in mind.
case "$CHAIN_OUT" in
  *"are STALE"*) no "an unevaluable binary is not misreported as stale" \
                    "no stale message" "$CHAIN_OUT" ;;
  *) ok "an unevaluable binary is not misreported as stale" ;;
esac
drop_fixture

# --- the operator override: mtime axis only, allowed on a real run, announced ------------------
FIXTURE_AGE_BIN=calendar-refresh
CHAIN_ENV=("LS_SM_ALLOW_STALE_BINARIES=1")
run_chain --dry-run
assert_eq "the override lets a deliberately pinned stale binary through" "0" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"mtime axis is BYPASSED for this run"*)
    ok "the override announces the bypass in the transcript rather than passing silently" ;;
  *) no "the override announces the bypass in the transcript rather than passing silently" \
        "a 'mtime axis is BYPASSED for this run' banner" "$CHAIN_OUT" ;;
esac
# Named per binary too, so the transcript records exactly which artifacts the operator vouched for.
case "$CHAIN_OUT" in
  *"PIN "*calendar-refresh*) ok "the override names each pinned binary individually" ;;
  *) no "the override names each pinned binary individually" \
        "a PIN verdict line for calendar-refresh" "$CHAIN_OUT" ;;
esac
drop_fixture

# The override's reach STOPS at the stale-by-mtime verdict. "Stale" is a known state an operator
# can pin on purpose — that is the whole justification for the escape — while "unevaluable" is the
# preflight not knowing WHAT it is about to run, which no operator assertion covers.
FIXTURE_DROP_DEPINFO_FOR=calendar-status
CHAIN_ENV=("LS_SM_ALLOW_STALE_BINARIES=1")
run_chain --dry-run
assert_eq "the override does NOT suppress the unevaluable refusal" "64" "$CHAIN_RC"
drop_fixture

CHAIN_ENV=("LS_SM_ALLOW_STALE_BINARIES=yes")
run_chain --dry-run
assert_eq "a malformed LS_SM_ALLOW_STALE_BINARIES is refused (64), not defaulted" "64" "$CHAIN_RC"
drop_fixture

# --- the content axis -------------------------------------------------------------------------
# mtime cannot see an INVERTED binary — newer than every source yet built from older code, which is
# what a build racing a git pull, a build in another worktree, or `touch target/debug/*` produces.
# The stub below is FRESH by mtime and simply lacks its registered guard, so the content axis is
# the only thing that can refuse it.
FIXTURE_OMIT_LITERAL_FOR=calendar-refresh
run_chain --dry-run
assert_eq "a fresh binary missing its REGISTERED guard literal refuses (64)" "64" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"missing their REGISTERED GUARD literal"*)
    ok "the literal-absent refusal is distinct from the absent and stale messages" ;;
  *) no "the literal-absent refusal is distinct from the absent and stale messages" \
        "a 'missing their REGISTERED GUARD literal' message" "$CHAIN_OUT" ;;
esac
# R7's containment for the reworded-source case: nothing runs `make script-check` automatically, so
# the refusal message itself is what lets an operator at 08:45 tell a reworded source from a stale
# binary in one line — and fix the registry rather than reach for the override.
case "$CHAIN_OUT" in
  *"BIN_PROBE_LITERALS entry 'calendar-refresh' expects:"*)
    ok "the literal-absent refusal names the registry entry" ;;
  *) no "the literal-absent refusal names the registry entry" \
        "a \"BIN_PROBE_LITERALS entry 'calendar-refresh' expects:\" line" "$CHAIN_OUT" ;;
esac
case "$CHAIN_OUT" in
  *"make script-check"*) ok "the literal-absent refusal names make script-check as the decider" ;;
  *) no "the literal-absent refusal names make script-check as the decider" \
        "make script-check named in the remedy" "$CHAIN_OUT" ;;
esac
drop_fixture

# NOT bypassable (R9). A binary pinned on purpose is still pinned to code containing its registered
# guard, so nothing legitimate needs that escape — and binding both axes to one switch would let
# the noisy axis train the operator into disabling the quiet one.
FIXTURE_OMIT_LITERAL_FOR=calendar-refresh
CHAIN_ENV=("LS_SM_ALLOW_STALE_BINARIES=1")
run_chain --dry-run
assert_eq "the override does NOT bypass the content axis" "64" "$CHAIN_RC"
drop_fixture

# ORDERING. A binary that is BOTH stale and missing its literal must report STALENESS: the literal
# failure is then merely a downstream symptom, and reporting it would send the operator to the
# registry when the answer is a rebuild.
FIXTURE_AGE_BIN=calendar-refresh
FIXTURE_OMIT_LITERAL_FOR=calendar-refresh
run_chain --dry-run
assert_eq "a stale AND literal-less binary still refuses (64)" "64" "$CHAIN_RC"
if [ -z "${CHAIN_OUT##*are STALE: calendar-refresh*}" ] \
   && [ -n "${CHAIN_OUT##*REGISTERED GUARD*}" ]; then
  ok "the content axis runs only after the mtime axis passes, so staleness is reported first"
else
  no "the content axis runs only after the mtime axis passes, so staleness is reported first" \
     "the stale message alone, with no REGISTERED GUARD message" "$CHAIN_OUT"
fi
drop_fixture

# --- R10: a registered literal that no longer occurs in the sources is a PERMANENT exit 64 -----
# Nothing runs `make script-check` automatically — it is not a gate-run step and no CI workflow
# invokes it — so this cannot PREEMPT a reword reaching the 08:45 chain. It makes it diagnosable.
REPO_ROOT="$(cd "$HERE/../../../.." && pwd)"
RUST_ROOTS=("$REPO_ROOT/adapters/nautilus/src" "$REPO_ROOT/adapters/nautilus/lab/src" \
            "$REPO_ROOT/adapters/nautilus/nautilus-ls-calendar/src" "$REPO_ROOT/crates")
while IFS='|' read -r RB RLIT _; do
  [ -n "$RB" ] || continue
  if grep -rqF --include='*.rs' -- "$RLIT" "${RUST_ROOTS[@]}"; then
    ok "registered literal for $RB still occurs in the repo's Rust sources"
  else
    no "registered literal for $RB still occurs in the repo's Rust sources" \
       "'$RLIT' present in some *.rs (a reword makes this a hard exit 64 at 08:45 — update BIN_PROBE_LITERALS)" \
       "not found under ${RUST_ROOTS[*]}"
  fi
done < <(registry_entries)

# The registry's FIELD COUNT, because `|` is the separator. A literal containing a pipe is silently
# truncated by probe_literal_for AND by registry_entries' own `IFS='|' read` — both sides would then
# agree on the same wrong value, so no behavioural assertion in this file could ever catch it. Only
# a structural check on the entry shape can.
REG_BAD=0
while IFS= read -r REG_LINE; do
  [ -n "$REG_LINE" ] || continue
  REG_FIELDS="$(printf '%s' "$REG_LINE" | awk -F'|' '{print NF}')"
  if [ "$REG_FIELDS" -ne 3 ]; then
    no "every BIN_PROBE_LITERALS entry has exactly 3 pipe-separated fields" \
       "3 fields (binary|literal|provenance) — a literal containing '|' is silently truncated" \
       "$REG_FIELDS fields in: $REG_LINE"
    REG_BAD=1
  fi
done < <(registry_entries)
[ "$REG_BAD" -eq 0 ] && ok "every BIN_PROBE_LITERALS entry has exactly 3 pipe-separated fields"

# And that assertion must itself be falsifiable: a fabricated entry has to fail it.
if grep -rqF --include='*.rs' -- 'REFUSED (asked for something no source ever says' "${RUST_ROOTS[@]}"; then
  no "the literal-drift assertion can see a fabricated registry entry" \
     "a fabricated literal to be absent from the Rust sources" "it was found"
else
  ok "the literal-drift assertion can see a fabricated registry entry"
fi

# --- NEGATIVE META-TESTS (R12): prove this harness can SEE each axis removed -------------------
# Without these the whole section is theatre — which is exactly how the missing --state-root
# survived the first version of this file.

# Neutralise the mtime COMPARISON, deliberately leaving the `|| vanished > 0` half standing so this
# mutant and the vanished one below are independent — the aged stub has all its sources, so the
# surviving half cannot rescue the refusal. Targeting the whole condition instead would make each
# mutant unable to distinguish a missing half from a working one.
FIXTURE_AGE_BIN=calendar-refresh
run_chain_mutated 's/bin_mtime < src_mtime/0 > 1/' --dry-run
assert_eq "harness detects a preflight stripped of the mtime freshness check" "0" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"are STALE"*) no "the mutated preflight no longer refuses the aged binary" \
                    "no stale refusal from the mutant" "$CHAIN_OUT" ;;
  *) ok "the mutated preflight no longer refuses the aged binary" ;;
esac
drop_fixture

# Strip ONLY the vanished-source clause, leaving the mtime comparison intact. This is what makes
# the two halves independently covered rather than jointly: the mutant above neutralises the whole
# condition, so on its own it could not tell a missing `|| vanished > 0` from a working one.
FIXTURE_DELETE_SRC_FOR=calendar-refresh
run_chain_mutated 's/ || vanished > 0//' --dry-run
assert_eq "harness detects the vanished-source clause stripped on its own" "0" "$CHAIN_RC"
drop_fixture

# De-register the probe literal. The literal-less stub must then sail through. Note the fixture
# still reads the REAL script's registry, so it plants what the mutant has stopped checking.
FIXTURE_OMIT_LITERAL_FOR=calendar-refresh
run_chain_mutated '/^  "calendar-refresh|REFUSED/d' --dry-run
assert_eq "harness detects a preflight whose probe-literal registry was emptied" "0" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"REGISTERED GUARD"*) no "the mutated preflight no longer refuses the literal-less binary" \
                           "no content-axis refusal from the mutant" "$CHAIN_OUT" ;;
  *) ok "the mutated preflight no longer refuses the literal-less binary" ;;
esac
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
#
# THE LAUNCH IS NOT ASSERTED HERE, deliberately. The stub's first act is to write its own
# start marker, and its TERM trap is installed on the LINE AFTER that. With LS_SM_POLL_SECS=1
# against an already-elapsed deadline the first poll kills roughly a second after launch, and
# bash startup can exceed that second — so SIGTERM legitimately arrives before the marker
# exists. A `*"ls-ingest "*)` arm reading that empty log as "never started" flaked ~1 run in 3
# (4 in 11, 2026-08-04), and any positive marker the stub writes races identically. The launch
# and the kill are proven instead by the two sibling assertions in this block, neither of which
# can race the stub's startup: exit 40 and `STAND DOWN — not on pace` are both emitted by the
# script itself, inside the LATE branch and after `kill "$ingest_pid"` (session-morning.sh:1012),
# and no other site can produce either on this fixture. What survives here is the one fact the
# race cannot fabricate: a COMPLETED marker means the kill did not land.
ingest_env 10 ""
CHAIN_ENV+=("LS_SM_UNIVERSE_BY=23:59")   # irrelevant: step [8] is unreachable from a kill
run_chain
assert_eq "normal mode: elapsed deadline + stalled ingest exits 40 (STAND-DOWN)" "40" "$CHAIN_RC"
case "$CHAIN_LOG" in
  *"ls-ingest COMPLETED"*)
    no "normal mode: the stalled ingest is killed" "no COMPLETED marker" "$CHAIN_LOG" ;;
  *) ok "normal mode: the stalled ingest is killed" ;;
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

# --- NEGATIVE META-TESTS: prove the guards above can be seen to fail -------------------
# Without these the assertions are unfalsifiable: a catch-up run that exits 41 for some
# unrelated reason would green both catch-up guards, and the normal-mode kill assertion is
# coverage-only — on an unmutated tree a green run proves nothing about what it would catch.

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

# Disarm the step [7] kill CALL, not the LATE branch — this is the permanent falsifier for the
# normal-mode kill assertion above, whose one surviving arm reds only on a COMPLETED marker.
# Deleting `kill "$ingest_pid"` and leaving `wait "$ingest_pid"` in place is what keeps the rest
# of that block still: `wait` now blocks until the 10s stub finishes, so the stub reaches its
# COMPLETED marker while the stand-down report and the exit code below it are untouched. Neutering
# the whole LATE branch would move all three at once and prove nothing about any one of them.
ingest_env 10 ""
CHAIN_ENV+=("LS_SM_UNIVERSE_BY=23:59")
run_chain_mutated 's|^    kill "\$ingest_pid" 2>/dev/null; |    |'
case "$CHAIN_LOG" in
  *"ls-ingest COMPLETED"*)
    ok "harness detects a step [7] LATE branch that no longer kills the ingest" ;;
  *) no "harness detects a step [7] LATE branch that no longer kills the ingest" \
        "an 'ls-ingest COMPLETED' marker — the surviving assertion's red condition" "$CHAIN_LOG" ;;
esac
assert_eq "the disarmed-kill mutation leaves the stand-down exit code unmoved" "40" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"STAND DOWN — not on pace"*)
    ok "the disarmed-kill mutation leaves the stand-down report unmoved" ;;
  *) no "the disarmed-kill mutation leaves the stand-down report unmoved" \
        "a 'STAND DOWN — not on pace' report" "$CHAIN_OUT" ;;
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
