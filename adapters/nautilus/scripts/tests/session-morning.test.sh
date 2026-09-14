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
# compiled binary with credentials stripped. For step [3]'s `calendar-fetch-inputs`
# that replay exercises the real `Args::parse` AND the real `confine()` state-root
# check, stopping at the credential refusal — before any HTTP client is constructed.
# For step [4]'s `calendar-refresh` it exercises the real `Args::parse` and stops at
# the snapshot-schema load, the first thing that binary does after parsing. Zero
# traffic on either: the second wires no HTTP client at all.
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
#   * The ORB step [10] (lab-mount-universe) and its step [11] GO/NO-GO report are still
#     never reached — the catch-up runs stop one step short of them by design, and nothing
#     here drives the attended path past the 09:00 guard. The DAILY-REHEARSAL [10] and [11]
#     are reached (U11, at the end of this file): that producer is offline, so no guard stands
#     in the way, and its argv plus [11]'s calendar-status --json argv are replayed for real.
#   * The freshness axes are exercised against STUBS, so they prove the SCRIPT's logic, not
#     that any real binary is current. Two assertions reach past the stubs to the artifacts
#     themselves: step [4]'s argv replay against the real `calendar-refresh`, and R11, which
#     greps every REGISTERED probe literal out of the real compiled binary it is registered
#     for. Nothing here still BUILDS anything — both require the artifact that gate step 6
#     (adapter-check) produced, and report loudly when it is absent.
#   * What remains uncovered is the MTIME axis against a real artifact: nothing here can tell
#     a current `target/debug` from one built before the last `git pull`. That is deliberate —
#     it is the preflight's job at 08:45, and reproducing it would mean this target owning a
#     build.
#   * `make script-check` runs as step 7 of `make gate-run` (after adapter-check, which
#     builds the binaries the argv replays and R11 need), so the R10/R11 assertions below
#     PREEMPT a reworded probe literal at the commit gate. No CI workflow invokes the
#     target, so a commit that never ran the gate can still carry a reword to the 08:45
#     chain as a hard exit 64 — the refusal message is what stays diagnosable there.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAL_SCRIPT="$HERE/../session-morning.sh"
REAL_BOOTSTRAP="$HERE/../rehearsal-bootstrap.sh"
# The prebuilt artifacts the replays and R11 read. Derived from ONE directory rather than
# spelled out per binary: R11 resolves a binary NAMED BY THE REGISTRY, so a second hardcoded
# path here could disagree with the one the replays use and the two would certify different
# artifacts under the same green.
REAL_BIN_DIR="$HERE/../../target/debug"
REAL_BIN="$REAL_BIN_DIR/calendar-fetch-inputs"
REAL_REFRESH_BIN="$REAL_BIN_DIR/calendar-refresh"

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
  mkdir -p "$src_bin" "$src_core" "$src_meta" "$naut/lab" "$naut/nautilus-ls-calendar" \
           "$root/crates/ls-sdk"
  printf '%s\n' 'fn main() {}' >"$src_core/lib.rs"
  printf '%s\n' 'fixture: true' >"$src_meta/fixture.yaml"
  # The MANIFESTS cargo's dep-info records nowhere, which the preflight folds in by hand because a
  # manifest-only change (dep bump, `cargo update`, feature flip, toolchain pin) dirties every
  # binary while leaving each recorded source older than it. The fixture must carry all of them:
  # they count toward `vanished` when absent, so a fixture missing one would mark every stub stale.
  local m
  for m in "$root/Cargo.toml" "$root/Cargo.lock" "$naut/Cargo.toml" "$naut/Cargo.lock" \
           "$naut/rust-toolchain.toml" "$naut/lab/Cargo.toml" "$naut/nautilus-ls-calendar/Cargo.toml" \
           "$root/crates/ls-sdk/Cargo.toml" "$root/crates/ls-core/Cargo.toml"; do
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
  # The bootstrap is always the REAL one, symlinked for the same reason: the daily fixture below is
  # built BY it, so a home the chain accepts is a home the bootstrap actually produces.
  ln -s "$REAL_BOOTSTRAP" "$naut/scripts/rehearsal-bootstrap.sh"

  # Step [1] reads this and skips the network probe entirely when it finds a
  # POSITIVE line for the session date. FIXTURE_SESSION_COMPACT moves it with a test that
  # overrides LS_SM_SESSION_DATE (a Monday reading Friday, a holiday).
  printf '%s\n' \
    "2026-07-31T08:20:42+0900 basDd=${FIXTURE_SESSION_COMPACT:-$SESSION_COMPACT} attempt=17 http=200 bytes=293133 rows=943 verdict=POSITIVE" \
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
  for b in calendar-activate lab-research; do
    cat >"$bin/$b" <<STUB
#!/usr/bin/env bash
echo "$b \$*" >>"\$STUB_LOG"
exit 0
STUB
    chmod +x "$bin/$b"
  done

  # calendar-status: step [6] calls it without --json and reads only its exit code. The daily
  # profile's step [11] calls it with --json per day and reads `day_status`, so the stub answers
  # that from a fixture calendar: weekdays are trading sessions, weekends are closed, and
  # STUB_CAL_OVERRIDES ("YYYY-MM-DD=status,...") marks holidays and Unknown days. STUB_CAL_FAIL=1
  # makes every --json answer an unusable load, the arm where the book cannot be judged at all.
  cat >"$bin/calendar-status" <<'STUB'
#!/usr/bin/env bash
echo "calendar-status $*" >>"$STUB_LOG"
day=""; json=0
while [ $# -gt 0 ]; do
  case "$1" in
    --day) day="$2"; shift 2 ;;
    --json) json=1; shift ;;
    *) shift ;;
  esac
done
[ "$json" = 1 ] || exit 0
[ "${STUB_CAL_FAIL:-0}" = 1 ] && { echo '{"outcome":{"load":"missing"}}'; exit 1; }
python3 -c "
import datetime,json,sys
day,overrides=sys.argv[1],sys.argv[2]
table=dict(p.split('=',1) for p in overrides.split(',') if p)
status=table.get(day) or ('trading_session' if datetime.date.fromisoformat(day).weekday()<5 else 'closed')
print(json.dumps({'outcome':'healthy','target_day':day,'day_status':status}))" "$day" "${STUB_CAL_OVERRIDES:-}"
STUB
  chmod +x "$bin/calendar-status"

  # lab-mount-universe: the ORB [10] is never reached here (the 09:00 guard), so for it the stub only
  # logs. The daily [10] IS reached, and its [11] reads the file, so under --daily the stub writes a
  # DailyUniverseFile-shaped object for LS_MOUNT_UNIVERSE_DATE. It logs the env it was handed on its
  # own line — the producer reads its whole config from the environment, so argv alone proves little.
  #   STUB_UNIVERSE_SESSION  the session_date to write instead (the wrong-session arm)
  #   STUB_UNIVERSE_RC       a non-zero exit, with no file written
  cat >"$bin/lab-mount-universe" <<'STUB'
#!/usr/bin/env bash
echo "lab-mount-universe $*" >>"$STUB_LOG"
echo "universe-env DATA_HOME=${LS_DATA_HOME:-} DATE=${LS_MOUNT_UNIVERSE_DATE:-} METADATA=${LS_MOUNT_UNIVERSE_METADATA:-} LANE=${LS_DISPATCH_LANE_ENV:-}" >>"$STUB_LOG"
daily=0; out=""
while [ $# -gt 0 ]; do
  case "$1" in
    --daily) daily=1; shift ;;
    --out) out="$2"; shift 2 ;;
    *) shift ;;
  esac
done
[ "${STUB_UNIVERSE_RC:-0}" = 0 ] || { echo "stub: refused"; exit "$STUB_UNIVERSE_RC"; }
if [ "$daily" = 1 ] && [ -n "$out" ]; then
  printf '{"session_date":"%s","ranking_signal":"momentum12x1","ranking_signal_is_placeholder":false,"warmup_bars":13,"universe_metadata_hash":"fixture","rows":[{"shcode":"100000","rank":0,"tradable":true},{"shcode":"100001","rank":1,"tradable":false}]}\n' \
    "${STUB_UNIVERSE_SESSION:-$LS_MOUNT_UNIVERSE_DATE}" >"$out"
fi
exit 0
STUB
  chmod +x "$bin/lab-mount-universe"

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
# The ingest's whole contract is its environment, so that is what gets logged — on a line of its own
# whose prefix does not contain "ls-ingest", so no existing "was the ingest reached" match moves.
nsyms="$(printf '%s' "${LS_INGEST_SYMBOLS:-}" | awk -F, '{print NF}')"
echo "ingest-env KIND=${LS_INGEST_KIND:-} MODE=${LS_INGEST_MODE:-} NSYMS=$nsyms CATALOG=${LS_INGEST_CATALOG:-} LOOKBACK=${LS_INGEST_LOOKBACK:-}" >>"$STUB_LOG"
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
# STUB_INGEST_SHIFT="<instrument>|<bar_type>=<YYYYMMDD>" models a shift the accumulate detected AND
# HEALED, which is the end state a real completed heal leaves: heal_daily clears the `shifted` mark and
# appends a RebaseEvent in the same save (src/ingest/mod.rs). Leaving the mark set instead — as this
# stub first did — is a state ls-ingest never produces, and it made the [11] warning look covered while
# the real healed case fell straight through it.
if [ -n "${STUB_INGEST_SHIFT:-}" ]; then
  python3 -c "
import json,sys
p,spec=sys.argv[1],sys.argv[2]
key,date=spec.split('=',1)
instrument,bar_type=key.split('|',1)
c=json.load(open(p))
c.get('shifted',{}).pop(key,None)
c.setdefault('rebase_events',[]).append(
    {'instrument':instrument,'bar_type':bar_type,'detected':date,'healed':date,'origin':'heal'})
json.dump(c,open(p,'w'))" "$LS_INGEST_CATALOG/ingest-checkpoint.json" "$STUB_INGEST_SHIFT"
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
      "$naut/nautilus-ls-calendar/Cargo.toml" \
      "$root/crates/ls-sdk/Cargo.toml" "$root/crates/ls-core/Cargo.toml"
    touch "$root/$FIXTURE_STALE_VIA"
  fi

  # ---- the two DAILY homes (U11), made only when a test asks ------------------------------------
  # FIXTURE_JUDGMENT_HOME=1 builds a frozen judgment home the way the real one is laid out: a
  # 352-symbol daily checkpoint, a parquet placeholder, the FROZEN-20260812 marker at the home root,
  # a stray ingest lock, and a runs/ tree. FIXTURE_DAILY=1 additionally runs the REAL bootstrap over
  # it, so the rehearsal home every daily test drives is exactly what an operator would get.
  #   FIXTURE_DAILY_WM   the judgment checkpoint's daily watermark (default the prior session)
  #   FIXTURE_SHIFTED    its `shifted` object, as JSON (default {})
  #   FIXTURE_BOOK       JSON to write over the bootstrapped empty book, or __absent__ to delete it
  if [ "${FIXTURE_JUDGMENT_HOME:-0}" = 1 ] || [ "${FIXTURE_DAILY:-0}" = 1 ]; then
    local jh="$root/data/next-daily-2016" shifted="${FIXTURE_SHIFTED:-}"
    [ -n "$shifted" ] || shifted='{}'
    mkdir -p "$jh/catalog/data/bar/fixture" "$jh/runs/fixture-run"
    python3 -c "
import json,sys
path,wm,shifted=sys.argv[1],sys.argv[2],json.loads(sys.argv[3])
marks={f'{100000+i:06d}.XKRX|1-DAY':wm for i in range(352)}
json.dump({'watermarks':marks,'gaps':[],'shifted':shifted},open(path,'w'))" \
      "$jh/catalog/ingest-checkpoint.json" "${FIXTURE_DAILY_WM:-20260729}" "$shifted"
    printf 'fixture bars\n' >"$jh/catalog/data/bar/fixture/part-0.parquet"
    printf 'fixture pin\n' >"$jh/catalog/universe-metadata-pin.json"
    printf 'stale\n' >"$jh/catalog/.ls-ingest.lock"
    printf 'FROZEN fixture\n' >"$jh/FROZEN-20260812"
    printf 'judgment run\n' >"$jh/runs/fixture-run/manifest.json"
  fi
  if [ "${FIXTURE_DAILY:-0}" = 1 ]; then
    bash "$naut/scripts/rehearsal-bootstrap.sh" >"$root/bootstrap.out" 2>&1
    printf '%s' "$?" >"$root/bootstrap.rc"
    if [ "${FIXTURE_BOOK:-}" = "__absent__" ]; then
      rm -f "$root/data/rehearsal-daily/rehearsal/book.json"
    elif [ -n "${FIXTURE_BOOK:-}" ]; then
      printf '%s\n' "$FIXTURE_BOOK" >"$root/data/rehearsal-daily/rehearsal/book.json"
    fi
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
  FIXTURE_SESSION_COMPACT=""; FIXTURE_JUDGMENT_HOME=""; FIXTURE_DAILY=""; FIXTURE_DAILY_WM=""
  FIXTURE_SHIFTED=""; FIXTURE_BOOK=""
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

# The same contract for STEP [4], against the binary that actually carries PR #258's
# forward-horizon guard. calendar-refresh was the structurally least-covered binary here:
# the argv replay reached only calendar-fetch-inputs, and every freshness axis below runs
# against stubs, so nothing proved the real parser accepts what step [4] marshals.
#
# THE ORACLE, and why it is a POSITIVE signal rather than "no parse error seen".
# `run()` (src/bin/calendar-refresh.rs:45-49) does exactly two things in order: `Args::parse`,
# then `KrxCalendar::load_from_path` on --active. So reaching the snapshot-schema error proves
# every required flag was present AND every value parsed — the RFC3339 --as-of, the YYYY-MM-DD
# --through, and a --mode inside the accepted set — because none of those can be reported late.
# It is also reached long before `write_candidate` (:75), so this replay mutates NOTHING in the
# fixture, and this binary wires no HTTP client at all, so there is zero traffic by construction.
#
# The fixture's active snapshot is a one-line stub that cannot satisfy the schema, which is what
# makes that error the reliable stopping point. If it ever became schema-valid the replay would
# run past it and this helper would report `rejected: <output>` — LOUD and wrong-looking rather
# than a silent green, which is the correct direction for the failure to point.
replay_real_refresh() { # full stub-log line args -> verdict on stdout
  local argv="$1" out
  [ -x "$REAL_REFRESH_BIN" ] || { printf 'nobinary'; return; }
  # Credentials stripped for the same reason as the fetch replay: --inputs is always passed, so
  # the live port is unreachable, but unsetting them makes "no credential ever reaches this
  # replay" structural rather than a property of the argv that happens to hold today.
  # shellcheck disable=SC2086  # fixture paths are mktemp-generated and space-free
  out="$(cd / && env -u LS_KRX_APPKEY -u LS_KASI_SERVICE_KEY \
           "$REAL_REFRESH_BIN" $argv 2>&1)"
  case "$out" in
    *"calendar snapshot is not valid JSON for the schema"*) printf 'accepted' ;;
    *) printf 'rejected: %s' "$(printf '%s' "$out" | tr '\n' ' ')" ;;
  esac
}

refresh_argv_from_log() { # stub log -> the calendar-refresh arguments
  printf '%s\n' "$1" | sed -n 's/^calendar-refresh //p' | head -1
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

# And step [4]'s argv against the REAL parser, exactly as step [3]'s is. The stub above accepts
# any argv containing --active and --through, so on its own it certifies nothing about the binary
# the 08:45 chain actually runs.
REFRESH_ARGV="$(refresh_argv_from_log "$CHAIN_LOG")"
if [ -z "$REFRESH_ARGV" ]; then
  no "step [4] argv is replayable against the real binary" "a logged argv" "$CHAIN_LOG"
else
  VERDICT="$(replay_real_refresh "$REFRESH_ARGV")"
  case "$VERDICT" in
    accepted) ok "real calendar-refresh accepts step [4]'s argv from a foreign CWD" ;;
    nobinary) no "real calendar-refresh accepts step [4]'s argv from a foreign CWD" \
                 "a compiled $REAL_REFRESH_BIN (run: cargo build --workspace --bin calendar-refresh)" \
                 "binary not built" ;;
    *)        no "real calendar-refresh accepts step [4]'s argv from a foreign CWD" \
                 "argument parsing to pass and the run to stop at the snapshot load" "$VERDICT" ;;
  esac
fi

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

# NEGATIVE META-TESTS for the step [4] replay. Same reasoning as step [3]'s: an assertion whose
# failure mode was never observed is theatre. Both sed expressions target the LIVE invocation
# only — `--mode incremental --through` and `--through "$session_date"` are single-line forms
# unique to session-morning.sh:863-866, while the --dry-run heredoc carries the same flags on
# separate unquoted lines, so neither mutant edits the printed prose instead of the real call.
run_chain_mutated 's/--mode incremental --through/--through/' --stop-before-activate
MUT_ARGV="$(refresh_argv_from_log "$CHAIN_LOG")"
if [ -z "$MUT_ARGV" ]; then
  no "harness detects a step [4] stripped of --mode" "a logged argv" "$CHAIN_LOG"
else
  case "$(replay_real_refresh "$MUT_ARGV")" in
    rejected*) ok "harness detects a step [4] stripped of --mode" ;;
    nobinary)  no "harness detects a step [4] stripped of --mode" \
                  "a compiled $REAL_REFRESH_BIN" "binary not built" ;;
    accepted)  no "harness detects a step [4] stripped of --mode" \
                  "the real binary to REFUSE the mutated argv" "it accepted it" ;;
  esac
fi
drop_fixture

# --through is the flag the forward-horizon guard reads (calendar-refresh.rs:98 compares it
# against the prior horizon), so a step [4] that stopped passing it would disarm the very
# refusal the content axis registers a literal for — while the stub kept logging happily.
run_chain_mutated 's/--through "\$session_date" //' --stop-before-activate
MUT_ARGV="$(refresh_argv_from_log "$CHAIN_LOG")"
if [ -z "$MUT_ARGV" ]; then
  no "harness detects a step [4] stripped of --through" "a logged argv" "$CHAIN_LOG"
else
  case "$(replay_real_refresh "$MUT_ARGV")" in
    rejected*) ok "harness detects a step [4] stripped of --through" ;;
    nobinary)  no "harness detects a step [4] stripped of --through" \
                  "a compiled $REAL_REFRESH_BIN" "binary not built" ;;
    accepted)  no "harness detects a step [4] stripped of --through" \
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
#
# The last two are the ROOT-CRATE manifests, and they are the reason the workspace-root Cargo.toml
# is not enough on its own. A feature default flipped in crates/ls-core/Cargo.toml dirties every
# binary while moving no lockfile at all — a lockfile records resolved VERSIONS, not feature sets —
# and `$R/Cargo.toml` carries only [workspace] members and shared version pins, not another crate's
# [features]. Those two crates are exactly the ones the seven binaries LINK; every other root member
# is a dev-dependency or a separate tool, and adding them would over-report.
#
# Note the case names say "dep-info lists no manifest", which holds for the five nautilus-ls
# binaries. lab-research and lab-mount-universe already carry these manifests through
# lab/build.rs's rerun-if-changed projection, so for them the entry is redundant rather than
# load-bearing — see the measured split in session-morning.sh's BIN_EXTRA_FRESHNESS_INPUTS comment.
for MANIFEST in Cargo.toml Cargo.lock adapters/nautilus/Cargo.toml adapters/nautilus/Cargo.lock \
                adapters/nautilus/rust-toolchain.toml adapters/nautilus/lab/Cargo.toml \
                adapters/nautilus/nautilus-ls-calendar/Cargo.toml \
                crates/ls-sdk/Cargo.toml crates/ls-core/Cargo.toml; do
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
# R7's containment for the reworded-source case: `make script-check` is a `make gate-run` step, but a
# commit that skipped the gate can still land a reword, so the refusal message itself is what lets an
# operator at 08:45 tell a reworded source from a stale binary in one line — and fix the registry
# rather than reach for the override.
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
# `make script-check` is step 7 of `make gate-run`, so this PREEMPTS a reword at the commit gate. No
# CI workflow invokes the target, so a gate-less commit can still carry one to the 08:45 chain — the
# refusal message below is what keeps that case diagnosable.
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

# --- R11: every registered literal must be in the REAL COMPILED ARTIFACT, not just the sources --
# R10 above proves the registry has not drifted from the Rust SOURCES; the freshness section
# proves the SCRIPT refuses a stub the fixture deliberately withheld the literal from. Neither
# touches the artifact the 08:45 chain actually greps — the fixture PLANTS the literal into its
# stub, so the content axis passes there by construction whatever `target/debug` contains.
#
# That gap is the exact shape of
# docs/solutions/workflow-issues/first-run-of-a-new-guard-prove-the-binary-then-discharge-its-residual.md:
# a guard's verdict certifies neither its own presence nor its inputs. This is the assertion that
# certifies its presence, and it is the same `grep -qaF` the preflight itself runs
# (session-morning.sh:446) — PRESENCE, never a count, because all three forward_horizon verdicts
# share a byte-identical prefix and a count is an artifact of literal merging (KTD4).
#
# The real binary is REQUIRED, reported loudly when absent exactly as the argv replay does. This
# target is `make gate-run` step 7, immediately after adapter-check, which is the step that builds
# these artifacts — so an absent binary means the gate was run out of order, not that the check is
# optional.
while IFS='|' read -r RB RLIT _; do
  [ -n "$RB" ] || continue
  if [ ! -x "$REAL_BIN_DIR/$RB" ]; then
    no "registered literal for $RB is present in the real compiled artifact" \
       "a compiled $REAL_BIN_DIR/$RB (run: cargo build --workspace --bin $RB, from adapters/nautilus)" \
       "binary not built"
  elif grep -qaF -- "$RLIT" "$REAL_BIN_DIR/$RB"; then
    ok "registered literal for $RB is present in the real compiled artifact"
  else
    no "registered literal for $RB is present in the real compiled artifact" \
       "'$RLIT' in $REAL_BIN_DIR/$RB — the preflight would refuse this binary at 08:45 (exit 64)" \
       "absent from the built artifact; rebuild it, or fix BIN_PROBE_LITERALS if the source was reworded"
  fi
done < <(registry_entries)

# Falsifiable in its own right: the grep must be able to return ABSENT against the same artifact.
# Without this a `grep` invocation that silently succeeded on everything would green the block
# above, which is the failure mode R10's fabricated-entry check exists to rule out for sources.
if [ ! -x "$REAL_REFRESH_BIN" ]; then
  no "the artifact-literal assertion can see a fabricated literal" \
     "a compiled $REAL_REFRESH_BIN" "binary not built"
elif grep -qaF -- 'REFUSED (asked for something no binary ever says' "$REAL_REFRESH_BIN"; then
  no "the artifact-literal assertion can see a fabricated literal" \
     "a fabricated literal to be absent from the built artifact" "it was found"
else
  ok "the artifact-literal assertion can see a fabricated literal"
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

# ============================================================ U11: profiles and the rehearsal home
# Plan 2026-09-08-1215 U11 (R11, R23, R25; KTD10, KTD11). The daily rehearsal accumulates a CLONE of
# the frozen judgment home and prepares the 15:20 close-auction session, through the same chain the
# ORB morning uses. Four properties are guarded below, each with a mutant that proves the harness
# can see it fail:
#   * the default profile is unchanged — explicit `orb` reproduces the default's argv exactly;
#   * a FROZEN home is refused before any traffic, in either profile;
#   * the daily profile hands ls-ingest the rehearsal catalog with every daily symbol, swaps [10]
#     for the OFFLINE `lab-mount-universe --daily`, and never runs lab-live;
#   * [11] judges the watermark and the book against the PROVEN calendar — a Monday reads Friday's
#     book, a holiday is skipped, a stamp between the proven session and today is healthy.

REAL_MOUNT_BIN="$REAL_BIN_DIR/lab-mount-universe"
REAL_STATUS_BIN="$REAL_BIN_DIR/calendar-status"
DAILY_HOME_REL="data/rehearsal-daily"

# `LS_SM_UNIVERSE_BY=23:59` is the only way the live daily path reaches [10] without a clock seam,
# and it would stand the run down inside the day's last minute. Wait that minute out rather than
# flake in it.
avoid_midnight_edge() {
  if [ "$(date +%H%M)" -ge 2358 ]; then sleep 125; fi
}

# A live daily run that reaches [11]: poll at 1s, deadlines far away, the stub ingest finishing at
# once and advancing every daily watermark to $1 (default the session date). Extra NAME=value
# overrides follow.
daily_live_env() { # [advance_to] [NAME=value...]
  local advance="${1:-$SESSION_COMPACT}"; shift || true
  CHAIN_ENV=(
    "LS_SM_PROFILE=daily-rehearsal"
    "LS_SM_POLL_SECS=1"
    "LS_SM_INGEST_BY=23:59"
    "LS_SM_UNIVERSE_BY=23:59"
    "STUB_INGEST_SECS=0"
    "STUB_INGEST_ADVANCE_TO=$advance"
    "$@"
  )
}

# The argv a stub logged for one binary, from the FIRST line whose prefix matches.
argv_from_log() { # log binary [must-contain]
  printf '%s\n' "$1" | grep -F -- "${3:-}" | sed -n "s/^$2 //p" | head -1
}

# Replay a daily-profile argv against a REAL binary from a foreign CWD, every LS_* already unset by
# this harness. Each oracle is a POSITIVE signal reached only after the binary's argument parser
# accepted everything:
#   * lab-mount-universe parses its whole argv before reading any environment, so "LS_DATA_HOME is
#     required" is reached ONLY when every argument was accepted — and it stops there, before any
#     catalog read.
#   * calendar-status on the fixture's schema-invalid snapshot lands on the typed `"load": "corrupt"`
#     diagnostic, which it only renders after Args::parse succeeded; a bad flag prints `unknown
#     argument` instead.
replay_real_daily() { # binary sentinel argv -> verdict
  local bin="$1" sentinel="$2" argv="$3" out
  [ -x "$bin" ] || { printf 'nobinary'; return; }
  # shellcheck disable=SC2086  # fixture paths are mktemp-generated and space-free
  out="$(cd / && "$bin" $argv 2>&1)"
  case "$out" in
    *"$sentinel"*) printf 'accepted' ;;
    *) printf 'rejected: %s' "$(printf '%s' "$out" | tr '\n' ' ')" ;;
  esac
}
replay_real_mount()  { replay_real_daily "$REAL_MOUNT_BIN" "LS_DATA_HOME is required" "$1"; }
replay_real_status() { replay_real_daily "$REAL_STATUS_BIN" '"load": "corrupt"' "$1"; }

# --- the Rust constants both scripts restate --------------------------------------------------
# The marker name, the book version and the ordinal epoch are Rust constants, and both scripts carry
# them as literals — one to exclude and refuse the marker, the other to write and judge the book. A
# changed constant with a stale literal would clone the marker, or judge every book as foreign.
rust_const() { # file const -> value
  sed -n "s/^pub const $2: [^=]*= \"\{0,1\}\([^\";]*\)\"\{0,1\};$/\1/p" "$REPO_ROOT/$1" | head -1
}
script_var() { # script var -> value
  sed -n "s/^$2=\"\{0,1\}\([^\"]*\)\"\{0,1\}$/\1/p" "$1" | head -1
}
RUST_MARKER="$(rust_const adapters/nautilus/src/ingest/mod.rs FROZEN_CATALOG_MARKER)"
RUST_BOOK_VERSION="$(rust_const adapters/nautilus/lab/src/runner/live_daily.rs BOOK_VERSION)"
RUST_EPOCH="$(rust_const adapters/nautilus/lab/src/runner/live_daily.rs SESSION_ORDINAL_EPOCH)"
# The parity assertions below fail closed when a constant is ABSENT (the value becomes the literal
# '<rust const not found>'). This proves they also see a constant that DRIFTED: the same reader, run
# over a copy whose literal was changed, must disagree with the Rust source.
DRIFTED="$(mktemp)"
/usr/bin/sed 's/^BOOK_VERSION=2$/BOOK_VERSION=3/' "$REAL_SCRIPT" >"$DRIFTED"
if [ "$(script_var "$DRIFTED" BOOK_VERSION)" = "$RUST_BOOK_VERSION" ]; then
  no "the constant-parity check can see a DRIFTED literal, not only an absent one" \
     "a changed BOOK_VERSION to differ from the Rust constant" "both read $RUST_BOOK_VERSION"
else
  ok "the constant-parity check can see a DRIFTED literal, not only an absent one"
fi
rm -f "$DRIFTED"

for S in "$REAL_SCRIPT" "$REAL_BOOTSTRAP"; do
  SN="${S##*/}"
  assert_eq "$SN's FROZEN_MARKER matches nautilus_ls::ingest::FROZEN_CATALOG_MARKER" \
            "${RUST_MARKER:-<rust const not found>}" "$(script_var "$S" FROZEN_MARKER)"
  assert_eq "$SN's BOOK_VERSION matches live_daily::BOOK_VERSION" \
            "${RUST_BOOK_VERSION:-<rust const not found>}" "$(script_var "$S" BOOK_VERSION)"
  assert_eq "$SN's SESSION_ORDINAL_EPOCH matches live_daily::SESSION_ORDINAL_EPOCH" \
            "${RUST_EPOCH:-<rust const not found>}" "$(script_var "$S" SESSION_ORDINAL_EPOCH)"
done

# --- rehearsal-bootstrap.sh: the clone ---------------------------------------------------------
FIXTURE_DAILY=1
CHAIN_ROOT="$(make_fixture)"
assert_eq "bootstrap: creates the rehearsal home (0)" "0" "$(cat "$CHAIN_ROOT/bootstrap.rc")"
DH="$CHAIN_ROOT/$DAILY_HOME_REL"
JH="$CHAIN_ROOT/data/next-daily-2016"
if cmp -s "$JH/catalog/ingest-checkpoint.json" "$DH/catalog/ingest-checkpoint.json"; then
  ok "bootstrap: the ingest checkpoint is cloned byte-for-byte"
else
  no "bootstrap: the ingest checkpoint is cloned byte-for-byte" "identical checkpoints" "they differ or one is missing"
fi
if [ -f "$DH/catalog/data/bar/fixture/part-0.parquet" ] && [ -f "$DH/catalog/universe-metadata-pin.json" ]; then
  ok "bootstrap: the catalog's bars and metadata pin are cloned"
else
  no "bootstrap: the catalog's bars and metadata pin are cloned" "both files in the clone" "$(find "$DH" -type f)"
fi
assert_eq "bootstrap: the clone carries NO FROZEN marker, so it can advance" "" \
          "$(find "$DH" -name 'FROZEN-*')"
assert_eq "bootstrap: the clone carries NO lock file" "" "$(find "$DH" -name '*.lock')"
assert_eq "bootstrap: the judgment runs/ are not copied" "absent" "$([ -e "$DH/runs" ] && echo present || echo absent)"
assert_eq "bootstrap: state/ exists for the spend ledger" "yes" "$([ -d "$DH/state" ] && echo yes || echo no)"
assert_eq "bootstrap: the source keeps its marker" "yes" "$([ -f "$JH/FROZEN-20260812" ] && echo yes || echo no)"
# The empty book must be the one the runner restores as a flat start: RehearsalBook::empty("").
BOOK_SHAPE="$(python3 -c "
import json,sys
b=json.load(open(sys.argv[1]))
print(sorted(b), b['version'], repr(b['session_date']), repr(b['run_id']), b['ordinal_epoch'], b['legs'])" \
  "$DH/rehearsal/book.json" 2>&1)"
assert_eq "bootstrap: book.json is RehearsalBook::empty(\"\")" \
          "['legs', 'ordinal_epoch', 'run_id', 'session_date', 'version'] $RUST_BOOK_VERSION '' '' $RUST_EPOCH []" \
          "$BOOK_SHAPE"
# And every key it writes is a field of the Rust struct, so a renamed field cannot pass on values.
BOOK_RS="$REPO_ROOT/adapters/nautilus/lab/src/runner/live_daily/book.rs"
for FIELD in version session_date run_id ordinal_epoch legs; do
  if grep -qE "^    pub $FIELD: " "$BOOK_RS"; then
    ok "bootstrap: book key '$FIELD' is a RehearsalBook field"
  else
    no "bootstrap: book key '$FIELD' is a RehearsalBook field" "pub $FIELD: in $BOOK_RS" "not found"
  fi
done

# Never overwrites: a second run refuses and leaves the home — and any book in it — untouched.
printf '%s\n' '{"sentinel":true}' >"$DH/rehearsal/book.json"
bash "$CHAIN_ROOT/adapters/nautilus/scripts/rehearsal-bootstrap.sh" >/dev/null 2>&1
assert_eq "bootstrap: an existing rehearsal home is refused (64)" "64" "$?"
assert_eq "bootstrap: the refused re-run leaves the existing book untouched" '{"sentinel":true}' \
          "$(cat "$DH/rehearsal/book.json")"
drop_fixture

FIXTURE_JUDGMENT_HOME=1
CHAIN_ROOT="$(make_fixture)"
bash "$CHAIN_ROOT/adapters/nautilus/scripts/rehearsal-bootstrap.sh" --dry-run >/dev/null 2>&1
assert_eq "bootstrap: --dry-run exits 0" "0" "$?"
assert_eq "bootstrap: --dry-run writes nothing" "absent" \
          "$([ -e "$CHAIN_ROOT/$DAILY_HOME_REL" ] && echo present || echo absent)"
# A symlink in the source would be copied as a link a later accumulate writes THROUGH.
ln -s /tmp "$CHAIN_ROOT/data/next-daily-2016/catalog/escape"
bash "$CHAIN_ROOT/adapters/nautilus/scripts/rehearsal-bootstrap.sh" >/dev/null 2>&1
assert_eq "bootstrap: a symlink inside the source catalog is refused (64)" "64" "$?"
assert_eq "bootstrap: the symlink refusal publishes nothing" "absent" \
          "$([ -e "$CHAIN_ROOT/$DAILY_HOME_REL" ] && echo present || echo absent)"
drop_fixture

CHAIN_ROOT="$(make_fixture)"
bash "$CHAIN_ROOT/adapters/nautilus/scripts/rehearsal-bootstrap.sh" >/dev/null 2>&1
assert_eq "bootstrap: a missing judgment home is refused (64)" "64" "$?"
bash "$CHAIN_ROOT/adapters/nautilus/scripts/rehearsal-bootstrap.sh" --bogus >/dev/null 2>&1
assert_eq "bootstrap: an unknown argument is refused (64)" "64" "$?"
drop_fixture

# --- the profile switch itself -------------------------------------------------------------------
CHAIN_ENV=("LS_SM_PROFILE=daily")
run_chain --dry-run
assert_eq "an unknown LS_SM_PROFILE is refused (64), not defaulted to orb" "64" "$CHAIN_RC"
drop_fixture
CHAIN_ENV=("LS_SM_DATA_HOME=data/turn4-fresh")
run_chain --dry-run
assert_eq "a relative LS_SM_DATA_HOME is refused (64)" "64" "$CHAIN_RC"
drop_fixture

# --- the default profile is unchanged --------------------------------------------------------------
# Every pre-U11 assertion above already runs the default. What this adds is that NAMING the orb
# profile changes nothing: the same catch-up chain, default and explicit, marshals the same argv to
# every binary in the same order and exits the same way. Root paths and the per-run --as-of stamp
# are the only differences normalised away.
normalise_log() { # root log
  printf '%s\n' "$2" | sed -e "s|$1|<ROOT>|g" -e 's/--as-of [^ ]*/--as-of <T>/g'
}
ingest_env 0 "$SESSION_COMPACT"
run_chain --catch-up
DEFAULT_RC="$CHAIN_RC"; DEFAULT_LOG="$(normalise_log "$CHAIN_ROOT" "$CHAIN_LOG")"
drop_fixture
ingest_env 0 "$SESSION_COMPACT"
CHAIN_ENV+=("LS_SM_PROFILE=orb")
run_chain --catch-up
assert_eq "explicit orb: exits exactly as the default profile does" "$DEFAULT_RC" "$CHAIN_RC"
assert_eq "explicit orb: marshals exactly the default profile's argv, in order" \
          "$DEFAULT_LOG" "$(normalise_log "$CHAIN_ROOT" "$CHAIN_LOG")"
case "$CHAIN_LOG" in
  *"ingest-env KIND=daily MODE=accumulate NSYMS=2 CATALOG=$CHAIN_ROOT/data/turn4-fresh/catalog LOOKBACK=20260518"*)
    ok "orb: ls-ingest still gets the turn4-fresh catalog and the 20260518 lookback" ;;
  *) no "orb: ls-ingest still gets the turn4-fresh catalog and the 20260518 lookback" \
        "an ingest-env line for turn4-fresh / 20260518" "$CHAIN_LOG" ;;
esac
drop_fixture

# The clocks now come from the profile table rather than from inline defaults, so pin both arms. The
# equivalence test above cannot: it overrides the clocks to make itself deterministic, so a mutant
# moving the ORB arm's 09:05/09:10 would pass it.
run_chain --dry-run
case "$CHAIN_OUT" in
  *"ingest by 09:05, universe by 09:10"*) ok "orb: the 09:05 / 09:10 clocks are the default" ;;
  *) no "orb: the 09:05 / 09:10 clocks are the default" "ingest by 09:05, universe by 09:10" "$CHAIN_OUT" ;;
esac
drop_fixture
FIXTURE_DAILY=1
CHAIN_ENV=("LS_SM_PROFILE=daily-rehearsal")
run_chain --dry-run
case "$CHAIN_OUT" in
  *"ingest by 15:00, universe by 15:10"*) ok "daily: the 15:00 / 15:10 clocks precede the 15:15 cutoff" ;;
  *) no "daily: the 15:00 / 15:10 clocks precede the 15:15 cutoff" "ingest by 15:00, universe by 15:10" "$CHAIN_OUT" ;;
esac
drop_fixture

# --- a FROZEN home is refused before any traffic, in either profile ------------------------------
for PROFILE in orb daily-rehearsal; do
  FIXTURE_JUDGMENT_HOME=1
  CHAIN_ROOT="$(make_fixture)"
  CHAIN_ENV=("LS_SM_PROFILE=$PROFILE" "LS_SM_DATA_HOME=$CHAIN_ROOT/data/next-daily-2016")
  _run_in "$CHAIN_ROOT"
  assert_eq "$PROFILE: LS_SM_DATA_HOME at the frozen judgment home is refused (64)" "64" "$CHAIN_RC"
  case "$CHAIN_OUT" in
    *"FROZEN-20260812 exists"*) ok "$PROFILE: the refusal names the marker" ;;
    *) no "$PROFILE: the refusal names the marker" "a 'FROZEN-20260812 exists' message" "$CHAIN_OUT" ;;
  esac
  assert_eq "$PROFILE: the frozen-home refusal issues no traffic" "" "$CHAIN_LOG"
  drop_fixture
done

# The Rust guard also reads a marker misplaced INSIDE catalog/; so does this one.
FIXTURE_DAILY=1
CHAIN_ROOT="$(make_fixture)"
printf 'misplaced\n' >"$CHAIN_ROOT/$DAILY_HOME_REL/catalog/FROZEN-20260812"
CHAIN_ENV=("LS_SM_PROFILE=daily-rehearsal")
_run_in "$CHAIN_ROOT" --dry-run
assert_eq "a marker misplaced inside catalog/ is refused too (64)" "64" "$CHAIN_RC"
drop_fixture

# NEGATIVE META-TEST: empty the marker loop and the orb run against the frozen home sails through.
FIXTURE_JUDGMENT_HOME=1
CHAIN_ROOT="$(make_fixture 's|^  if \[\[ -e "\$marker" \]\]; then$|  if false; then|')"
CHAIN_ENV=("LS_SM_DATA_HOME=$CHAIN_ROOT/data/next-daily-2016")
_run_in "$CHAIN_ROOT" --dry-run
assert_eq "harness detects a preflight stripped of the frozen-home refusal" "0" "$CHAIN_RC"
drop_fixture

# --- a LINKED catalog is the hole the logical marker checks leave -------------------------------
# The marker sits at the judgment home's ROOT, so a hand-built home whose catalog/ links into that
# home carries no marker on either logical path the preflight (or the Rust write guard) inspects,
# while the accumulate writes through the link into the frozen bars.
FIXTURE_DAILY=1
CHAIN_ROOT="$(make_fixture)"
LINKED="$CHAIN_ROOT/data/linked-home"
mkdir -p "$LINKED/rehearsal"
ln -s "$CHAIN_ROOT/data/next-daily-2016/catalog" "$LINKED/catalog"
printf '%s\n' '{"version":2,"session_date":"","run_id":"","ordinal_epoch":"2010-01-04","legs":[]}' \
  >"$LINKED/rehearsal/book.json"
CHAIN_ENV=("LS_SM_PROFILE=daily-rehearsal" "LS_SM_DATA_HOME=$LINKED")
_run_in "$CHAIN_ROOT" --dry-run
assert_eq "a catalog/ symlinked into the frozen home is refused (64)" "64" "$CHAIN_RC"
assert_eq "the symlinked-catalog refusal issues no traffic" "" "$CHAIN_LOG"
case "$CHAIN_OUT" in
  *"is a symlink"*) ok "the symlink refusal names the link rather than the marker" ;;
  *) no "the symlink refusal names the link rather than the marker" "an 'is a symlink' message" "$CHAIN_OUT" ;;
esac
# And with the link resolved away (a real directory whose REAL path is the frozen catalog — a bind
# mount, a moved home, a hard-linked tree), the realpath marker check is what refuses.
rm "$LINKED/catalog"
cp -Rp "$CHAIN_ROOT/data/next-daily-2016/catalog" "$LINKED/catalog"
cp "$CHAIN_ROOT/data/next-daily-2016/FROZEN-20260812" "$LINKED/catalog/FROZEN-20260812"
_run_in "$CHAIN_ROOT" --dry-run
assert_eq "a marker reachable only through the resolved catalog path still refuses (64)" "64" "$CHAIN_RC"
drop_fixture

# NEGATIVE META-TEST: strip the symlink refusal and the linked home sails into the chain.
FIXTURE_DAILY=1
CHAIN_ROOT="$(make_fixture 's/^  if \[\[ -L "\$leaf" \]\]; then$/  if false; then/')"
LINKED="$CHAIN_ROOT/data/linked-home"
mkdir -p "$LINKED/rehearsal"
ln -s "$CHAIN_ROOT/data/rehearsal-daily/catalog" "$LINKED/catalog"
printf '%s\n' '{"version":2,"session_date":"","run_id":"","ordinal_epoch":"2010-01-04","legs":[]}' \
  >"$LINKED/rehearsal/book.json"
CHAIN_ENV=("LS_SM_PROFILE=daily-rehearsal" "LS_SM_DATA_HOME=$LINKED")
_run_in "$CHAIN_ROOT" --dry-run
assert_eq "harness detects a preflight stripped of the symlinked-home refusal" "0" "$CHAIN_RC"
drop_fixture

# A home without rehearsal/ is not a bootstrapped rehearsal home — most likely an ORB minute home.
CHAIN_ROOT="$(make_fixture)"
CHAIN_ENV=("LS_SM_PROFILE=daily-rehearsal" "LS_SM_DATA_HOME=$CHAIN_ROOT/data/turn4-fresh")
_run_in "$CHAIN_ROOT" --dry-run
assert_eq "daily: a home with no rehearsal/ is refused (64)" "64" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"rehearsal-bootstrap.sh"*) ok "daily: the not-bootstrapped refusal names the bootstrap script" ;;
  *) no "daily: the not-bootstrapped refusal names the bootstrap script" "rehearsal-bootstrap.sh named" "$CHAIN_OUT" ;;
esac
drop_fixture

# --- the daily dry run -------------------------------------------------------------------------------
FIXTURE_DAILY=1
CHAIN_ENV=("LS_SM_PROFILE=daily-rehearsal")
run_chain --dry-run
assert_eq "daily --dry-run exits 0 on a freshly bootstrapped home" "0" "$CHAIN_RC"
assert_eq "daily --dry-run invokes no binary" "" "$CHAIN_LOG"
for WANT in "lab-mount-universe --daily --out $CHAIN_ROOT/$DAILY_HOME_REL/rehearsal/daily-universe-20260731.json" \
            "LS_INGEST_CATALOG=$CHAIN_ROOT/$DAILY_HOME_REL/catalog" \
            "LS_INGEST_LOOKBACK=20160801" \
            "ok: lab-live is never invoked"; do
  case "$CHAIN_OUT" in
    *"$WANT"*) ok "daily --dry-run shows: $WANT" ;;
    *) no "daily --dry-run shows: $WANT" "$WANT" "$CHAIN_OUT" ;;
  esac
done
case "$CHAIN_OUT" in
  *"only after 09:00"*) no "daily --dry-run carries no 09:00 guard (the producer is offline)" \
                           "no 09:00 guard text" "$CHAIN_OUT" ;;
  *) ok "daily --dry-run carries no 09:00 guard (the producer is offline)" ;;
esac
drop_fixture

# --- the daily live path, GO ---------------------------------------------------------------------------
avoid_midnight_edge
FIXTURE_DAILY=1
daily_live_env
run_chain
assert_eq "daily: a fresh bootstrapped home reaches GO (0)" "0" "$CHAIN_RC"
case "$CHAIN_LOG" in
  *"ingest-env KIND=daily MODE=accumulate NSYMS=352 CATALOG=$CHAIN_ROOT/$DAILY_HOME_REL/catalog LOOKBACK=20160801"*)
    ok "daily: ls-ingest accumulates all 352 daily symbols into the REHEARSAL catalog" ;;
  *) no "daily: ls-ingest accumulates all 352 daily symbols into the REHEARSAL catalog" \
        "ingest-env KIND=daily MODE=accumulate NSYMS=352 CATALOG=<rehearsal>/catalog LOOKBACK=20160801" "$CHAIN_LOG" ;;
esac
assert_eq "daily: [10] hands the producer the rehearsal home, the mount date, the metadata, and NO lane" \
          "universe-env DATA_HOME=$CHAIN_ROOT/$DAILY_HOME_REL DATE=2026-07-31 METADATA=$CHAIN_ROOT/adapters/nautilus/lab/config/universe-metadata-20260723.json LANE=" \
          "$(printf '%s\n' "$CHAIN_LOG" | grep '^universe-env ')"
case "$CHAIN_LOG" in
  *lab-live*) no "daily: lab-live is never invoked" "no lab-live call" "$CHAIN_LOG" ;;
  *) ok "daily: lab-live is never invoked" ;;
esac
case "$CHAIN_OUT" in
  *"LS_REHEARSAL_UNIVERSE_FILE=$CHAIN_ROOT/$DAILY_HOME_REL/rehearsal/daily-universe-20260731.json"*)
    ok "daily: the GO checklist names the universe file lab-live --rehearse-daily reads" ;;
  *) no "daily: the GO checklist names the universe file lab-live --rehearse-daily reads" \
        "LS_REHEARSAL_UNIVERSE_FILE=<rehearsal>/rehearsal/daily-universe-20260731.json" "$CHAIN_OUT" ;;
esac
case "$CHAIN_OUT" in
  *"previous proven trading session before 2026-07-31: 2026-07-30"*)
    ok "daily: [11] finds the previous proven session through calendar-status" ;;
  *) no "daily: [11] finds the previous proven session through calendar-status" \
        "previous proven ... 2026-07-30" "$CHAIN_OUT" ;;
esac

# Both new invocations replayed against the REAL binaries, from a foreign CWD.
MOUNT_ARGV="$(argv_from_log "$CHAIN_LOG" lab-mount-universe --daily)"
case "$(replay_real_mount "$MOUNT_ARGV")" in
  accepted) ok "real lab-mount-universe accepts the daily [10] argv" ;;
  nobinary) no "real lab-mount-universe accepts the daily [10] argv" "a compiled $REAL_MOUNT_BIN" "binary not built" ;;
  *) no "real lab-mount-universe accepts the daily [10] argv" "argument parsing to pass" "$(replay_real_mount "$MOUNT_ARGV")" ;;
esac
case "$(replay_real_mount "${MOUNT_ARGV/--daily/--dialy}")" in
  rejected*) ok "the [10] replay can see a misspelled flag" ;;
  nobinary) no "the [10] replay can see a misspelled flag" "a compiled $REAL_MOUNT_BIN" "binary not built" ;;
  *) no "the [10] replay can see a misspelled flag" "the real binary to refuse --dialy" "it accepted it" ;;
esac
STATUS_ARGV="$(argv_from_log "$CHAIN_LOG" calendar-status --json)"
case "$(replay_real_status "$STATUS_ARGV")" in
  accepted) ok "real calendar-status accepts the daily [11] --json argv" ;;
  nobinary) no "real calendar-status accepts the daily [11] --json argv" "a compiled $REAL_STATUS_BIN" "binary not built" ;;
  *) no "real calendar-status accepts the daily [11] --json argv" "argument parsing to pass" "$(replay_real_status "$STATUS_ARGV")" ;;
esac
case "$(replay_real_status "${STATUS_ARGV/--json/--jsn}")" in
  rejected*) ok "the [11] replay can see a misspelled flag" ;;
  nobinary) no "the [11] replay can see a misspelled flag" "a compiled $REAL_STATUS_BIN" "binary not built" ;;
  *) no "the [11] replay can see a misspelled flag" "the real binary to refuse --jsn" "it accepted it" ;;
esac
drop_fixture

# --- book freshness on the proven calendar ---------------------------------------------------------
# daily_book <stamp> [legs-json] -> a version-2 book stamped <stamp>
daily_book() {
  printf '{"version":2,"session_date":"%s","run_id":"r","ordinal_epoch":"2010-01-04","legs":%s}' "$1" "${2:-[]}"
}

# daily_leg <shcode> <entry_date> -> one leg carrying every field the runner refuses a book without
# (RehearsalBook::validate_fields), so a leg fixture cannot accidentally test the leg-validation path.
daily_leg() {
  printf '{"shcode":"%s","quantity":10,"entry_price":10000.0,"stop_price":9000.0,"prior_close":10100,"entry_date":"%s","entered_under":"momentum12x1","opening_order_id":"o-%s"}' \
    "$1" "$2" "$1"
}

# A book stamped the previous proven session is the ordinary case.
avoid_midnight_edge
FIXTURE_DAILY=1; FIXTURE_BOOK="$(daily_book 2026-07-30)"
daily_live_env
run_chain
assert_eq "daily: a book stamped the previous proven session is GO (0)" "0" "$CHAIN_RC"
drop_fixture

# A stamp older than it means a teardown never wrote the book.
FIXTURE_DAILY=1; FIXTURE_BOOK="$(daily_book 2026-07-29)"
daily_live_env
run_chain
assert_eq "daily: a book stamped BEFORE the previous proven session is NO-GO (1)" "1" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"book is stamped 2026-07-29, BEFORE the previous proven session 2026-07-30"*)
    ok "daily: the stale-book refusal names both dates" ;;
  *) no "daily: the stale-book refusal names both dates" "stamped 2026-07-29, BEFORE ... 2026-07-30" "$CHAIN_OUT" ;;
esac
drop_fixture

# NEGATIVE META-TEST: strip the staleness comparison and the same stale book reads GO.
FIXTURE_DAILY=1; FIXTURE_BOOK="$(daily_book 2026-07-29)"
daily_live_env
run_chain_mutated 's/^        elif stamp < previous:$/        elif False:/'
assert_eq "harness detects a [11] stripped of the book-staleness comparison" "0" "$CHAIN_RC"
drop_fixture

# A Monday reads Friday's book: the weekend is not a session, so Friday IS the previous one.
avoid_midnight_edge
FIXTURE_DAILY=1; FIXTURE_SESSION_COMPACT=20260731; FIXTURE_DAILY_WM=20260730
FIXTURE_BOOK="$(daily_book 2026-07-31)"
daily_live_env 20260731 "LS_SM_SESSION_DATE=2026-07-31" "LS_SM_MOUNT_DATE=2026-08-03"
run_chain
assert_eq "daily: on a Monday, a book stamped the Friday before is GO (0)" "0" "$CHAIN_RC"
drop_fixture
FIXTURE_DAILY=1; FIXTURE_SESSION_COMPACT=20260731; FIXTURE_DAILY_WM=20260730
FIXTURE_BOOK="$(daily_book 2026-07-30)"
daily_live_env 20260731 "LS_SM_SESSION_DATE=2026-07-31" "LS_SM_MOUNT_DATE=2026-08-03"
run_chain
assert_eq "daily: on a Monday, a book stamped the Thursday before is NO-GO (1)" "1" "$CHAIN_RC"
drop_fixture

# After a holiday Monday the previous session is the Friday before the long weekend.
avoid_midnight_edge
FIXTURE_DAILY=1; FIXTURE_SESSION_COMPACT=20260814; FIXTURE_DAILY_WM=20260813
FIXTURE_BOOK="$(daily_book 2026-08-14)"
daily_live_env 20260814 "LS_SM_SESSION_DATE=2026-08-14" "LS_SM_MOUNT_DATE=2026-08-18" \
  "STUB_CAL_OVERRIDES=2026-08-17=closed"
run_chain
assert_eq "daily: after a holiday Monday, the Friday book is GO (0)" "0" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"previous proven trading session before 2026-08-18: 2026-08-14"*)
    ok "daily: the proven-session walk skips the holiday and the weekend" ;;
  *) no "daily: the proven-session walk skips the holiday and the weekend" "previous ... 2026-08-14" "$CHAIN_OUT" ;;
esac
drop_fixture

# The KRX witness is retrospective: yesterday can still read Unknown while its own teardown already
# stamped the book with it. A stamp BETWEEN the proven session and today is healthy.
avoid_midnight_edge
FIXTURE_DAILY=1; FIXTURE_BOOK="$(daily_book 2026-07-30)"
daily_live_env "$SESSION_COMPACT" "STUB_CAL_OVERRIDES=2026-07-30=unknown"
run_chain
assert_eq "daily: a book stamped an Unknown yesterday, after the proven session, is GO (0)" "0" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"before 2026-07-31: 2026-07-29"*) ok "daily: an Unknown day is not counted as proven" ;;
  *) no "daily: an Unknown day is not counted as proven" "previous ... 2026-07-29" "$CHAIN_OUT" ;;
esac
drop_fixture

# The absent book is a flat start (RehearsalBook::load); an unreadable or foreign one is not.
avoid_midnight_edge
FIXTURE_DAILY=1; FIXTURE_BOOK="__absent__"
daily_live_env
run_chain
assert_eq "daily: an absent book is a flat start, GO (0)" "0" "$CHAIN_RC"
drop_fixture
FIXTURE_DAILY=1; FIXTURE_BOOK='{"version":2,'
daily_live_env
run_chain
assert_eq "daily: an unparseable book is NO-GO (1)" "1" "$CHAIN_RC"
drop_fixture
FIXTURE_DAILY=1; FIXTURE_BOOK='{"version":1,"session_date":"2026-07-30","run_id":"r","ordinal_epoch":"2010-01-04","legs":[]}'
daily_live_env
run_chain
assert_eq "daily: a version-1 book is NO-GO (1), not restored from fields it may misread" "1" "$CHAIN_RC"
drop_fixture

# --- the watermark row reads the PROVEN calendar, not LS_SM_SESSION_DATE -------------------------
# The session date defaults to a hardcoded literal, so a run can be handed a stale one. Here the chain
# ingests 2026-07-29 cleanly for a 2026-07-31 mount — [7] is satisfied — but 2026-07-30 is proven, so
# the catalog is a session behind what the rehearsal will trade on.
FIXTURE_DAILY=1; FIXTURE_SESSION_COMPACT=20260729; FIXTURE_DAILY_WM=20260728
daily_live_env 20260729 "LS_SM_SESSION_DATE=2026-07-29"
run_chain
assert_eq "daily: watermarks behind the previous proven session are NO-GO (1)" "1" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"352/352 daily watermark(s) are behind the previous proven session 2026-07-30"*)
    ok "daily: the watermark refusal counts the symbols behind" ;;
  *) no "daily: the watermark refusal counts the symbols behind" "352/352 ... behind ... 2026-07-30" "$CHAIN_OUT" ;;
esac
drop_fixture

FIXTURE_DAILY=1; FIXTURE_SESSION_COMPACT=20260729; FIXTURE_DAILY_WM=20260728
daily_live_env 20260729 "LS_SM_SESSION_DATE=2026-07-29"
run_chain_mutated 's/^    elif daily\[0\] < compact_previous:$/    elif False:/'
assert_eq "harness detects a [11] stripped of the proven-session watermark check" "0" "$CHAIN_RC"
drop_fixture

# --- the universe file and the calendar must both answer ---------------------------------------------
FIXTURE_DAILY=1
daily_live_env "$SESSION_COMPACT" "STUB_UNIVERSE_SESSION=2026-07-30"
run_chain
assert_eq "daily: a universe file resolved for another session is NO-GO (1)" "1" "$CHAIN_RC"
drop_fixture
FIXTURE_DAILY=1
daily_live_env "$SESSION_COMPACT" "STUB_UNIVERSE_RC=3"
run_chain
assert_eq "daily: a failed lab-mount-universe --daily is NO-GO (1) — no flat-open exception" "1" "$CHAIN_RC"
drop_fixture
FIXTURE_DAILY=1
daily_live_env "$SESSION_COMPACT" "STUB_CAL_FAIL=1"
run_chain
assert_eq "daily: a calendar that cannot answer for the book is NO-GO (1)" "1" "$CHAIN_RC"
drop_fixture

# --- adjustment-basis shifts on held symbols are WARNING rows, not refusals ------------------------
avoid_midnight_edge
SHIFT_LEGS="[$(daily_leg 100000 2026-07-20),$(daily_leg 100001 2026-07-21),$(daily_leg 100002 2026-07-22)]"
FIXTURE_DAILY=1
FIXTURE_SHIFTED='{"100000.XKRX|1-DAY":"20260725"}'
FIXTURE_BOOK="$(daily_book 2026-07-30 "$SHIFT_LEGS")"
daily_live_env "$SESSION_COMPACT" "STUB_INGEST_SHIFT=100001.XKRX|1-DAY=20260730"
run_chain
assert_eq "daily: a held symbol's basis shift warns but does not refuse (0)" "0" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"WARNING: held 100000 "*"detected 20260725 (already on record, heal pending)"*)
    ok "daily: a mark still in shifted is reported as a pending heal" ;;
  *) no "daily: a mark still in shifted is reported as a pending heal" \
        "WARNING: held 100000 ... (already on record, heal pending)" "$CHAIN_OUT" ;;
esac
# THE CASE THE FIRST VERSION OF THIS WARNING COULD NOT SEE. A real accumulate that detects a split on a
# held leg also heals it, clearing the `shifted` mark and leaving only a rebase event — so a check that
# reads `shifted` alone stays silent on the one shift the operator most needs to hear about.
case "$CHAIN_OUT" in
  *"WARNING: held 100001 "*"was RE-BASED this run"*"detected 20260730, healed 20260730"*)
    ok "daily: a shift detected AND healed by this ingest is still reported" ;;
  *) no "daily: a shift detected AND healed by this ingest is still reported" \
        "WARNING: held 100001 ... was RE-BASED this run ... detected 20260730, healed 20260730" "$CHAIN_OUT" ;;
esac
case "$CHAIN_OUT" in
  *"held 100002 "*) no "daily: an unshifted held symbol raises no warning" "no row for 100002" "$CHAIN_OUT" ;;
  *) ok "daily: an unshifted held symbol raises no warning" ;;
esac
drop_fixture

# NEGATIVE META-TEST: neutralise the rebase-event comparison and the healed shift goes unreported —
# which is exactly the state this check was added to end.
FIXTURE_DAILY=1
FIXTURE_BOOK="$(daily_book 2026-07-30 "$SHIFT_LEGS")"
daily_live_env "$SESSION_COMPACT" "STUB_INGEST_SHIFT=100001.XKRX|1-DAY=20260730"
run_chain_mutated 's/^    if healed:$/    if False:/'
case "$CHAIN_OUT" in
  *"RE-BASED this run"*)
    no "harness detects a [11] stripped of the rebase-event comparison" \
       "no RE-BASED row from the mutant" "$CHAIN_OUT" ;;
  *) ok "harness detects a [11] stripped of the rebase-event comparison" ;;
esac
drop_fixture

# --- the book's legs are judged the way the runner judges them --------------------------------------
# A GO on a book the mount then refuses sends the operator to the gateway at 15:15 to find out.
FIXTURE_DAILY=1
FIXTURE_BOOK="$(daily_book 2026-07-30 '[{"shcode":"100000","quantity":10,"entry_price":10000.0,"stop_price":10500.0,"prior_close":10100,"entry_date":"2026-07-20","entered_under":"momentum12x1","opening_order_id":"o-1"}]')"
daily_live_env
run_chain
assert_eq "daily: a leg whose stop is not below its entry is NO-GO (1)" "1" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"unusable leg(s) the runner would refuse"*"100000: stop_price"*)
    ok "daily: the unusable-leg refusal names the leg and the field" ;;
  *) no "daily: the unusable-leg refusal names the leg and the field" \
        "an 'unusable leg(s) ... 100000: stop_price' message" "$CHAIN_OUT" ;;
esac
drop_fixture
FIXTURE_DAILY=1
FIXTURE_BOOK="$(daily_book 2026-07-30 '[{"shcode":"100000","quantity":10,"entry_price":10000.0,"stop_price":9000.0,"prior_close":10100,"entry_date":"2026-07-20","entered_under":"momentum12x1"}]')"
daily_live_env
run_chain
assert_eq "daily: a leg with no opening_order_id is NO-GO (1)" "1" "$CHAIN_RC"
drop_fixture

# The two book branches the version-1 and staleness cases do not reach.
FIXTURE_DAILY=1
FIXTURE_BOOK='{"version":2,"session_date":"2026-07-30","run_id":"r","ordinal_epoch":"2009-01-02","legs":[]}'
daily_live_env
run_chain
assert_eq "daily: a book counting ordinals from another epoch is NO-GO (1)" "1" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"counts ordinals from"*) ok "daily: the epoch refusal names the mismatch" ;;
  *) no "daily: the epoch refusal names the mismatch" "a 'counts ordinals from' message" "$CHAIN_OUT" ;;
esac
drop_fixture
FIXTURE_DAILY=1
FIXTURE_BOOK="$(daily_book '' "[$(daily_leg 100000 2026-07-20)]")"
daily_live_env
run_chain
assert_eq "daily: legs with no session_date stamp are NO-GO (1)" "1" "$CHAIN_RC"
case "$CHAIN_OUT" in
  *"no session_date stamp"*) ok "daily: the unstamped-legs refusal names itself" ;;
  *) no "daily: the unstamped-legs refusal names itself" "a 'no session_date stamp' message" "$CHAIN_OUT" ;;
esac
drop_fixture

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ] || exit 1
