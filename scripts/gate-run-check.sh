#!/usr/bin/env bash
# Offline self-test for the resumable gate driver (plan 2026-07-29-002, U4; KTD4).
#
# Runs the REAL scripts/gate-run.sh end-to-end inside a throwaway git repo in
# mktemp, with fake `make`/`cargo` shims FIRST on PATH that log every invocation
# and return scripted exit codes (git stays real — the fingerprint logic under
# test is git-driven). No real gate step ever runs, no network, and this repo's
# own state is never touched (the driver resolves its repo root from CWD, which
# is the fixture).
#
# Scripted exit codes: the shim for `<tool> <args...>` looks up
#   $GRC_RC_DIR/<tool>_<args-with-spaces-as-underscores>
# and exits with that file's content (default 0). Content `block` makes the
# shim spin until $GRC_RC_DIR/unblock exists (for the live-concurrency case).
#
# Usage: scripts/gate-run-check.sh   (exit 0 = driver behaves; non-0 = regression)
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
GATE="$repo_root/scripts/gate-run.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

LOG="$work/invocations.log"
RC_DIR="$work/rc"
mkdir -p "$RC_DIR" "$work/bin"

# --- make/cargo shims: log invocation, return scripted exit code (no network) --
cat > "$work/bin/make" <<'SHIM'
#!/bin/sh
echo "make $*" >> "$GRC_LOG"
# Env-scrub canary (case H): the driver must strip LS_* from step envs.
if [ -n "${LS_PROBE_CANARY:-}" ]; then echo "make $* saw LS_PROBE_CANARY" >> "$GRC_RC_DIR/canary-leak"; fi
key="make_$(printf '%s' "$*" | tr ' ' '_')"
if [ -f "$GRC_RC_DIR/$key" ]; then
  v="$(cat "$GRC_RC_DIR/$key")"
  if [ "$v" = "block" ]; then
    i=0
    while [ ! -f "$GRC_RC_DIR/unblock" ] && [ "$i" -lt 100 ]; do sleep 0.1; i=$((i+1)); done
    exit 0
  fi
  exit "$v"
fi
exit 0
SHIM
cat > "$work/bin/cargo" <<'SHIM'
#!/bin/sh
echo "cargo $*" >> "$GRC_LOG"
# Env-scrub canary (case H): the driver must strip LS_* from step envs.
if [ -n "${LS_PROBE_CANARY:-}" ]; then echo "cargo $* saw LS_PROBE_CANARY" >> "$GRC_RC_DIR/canary-leak"; fi
key="cargo_$(printf '%s' "$*" | tr ' ' '_')"
if [ -f "$GRC_RC_DIR/$key" ]; then
  v="$(cat "$GRC_RC_DIR/$key")"
  if [ "$v" = "block" ]; then
    i=0
    while [ ! -f "$GRC_RC_DIR/unblock" ] && [ "$i" -lt 100 ]; do sleep 0.1; i=$((i+1)); done
    exit 0
  fi
  exit "$v"
fi
exit 0
SHIM
chmod +x "$work/bin/make" "$work/bin/cargo"

fails=0
ok()   { echo "ok[$1]: $2"; }
fail() { echo "FAIL[$1]: $2"; fails=1; }

fix=""
new_fixture() {
  fix="$work/repo-$1"
  rm -rf "$fix"
  mkdir -p "$fix"
  (
    cd "$fix" \
      && git init -q 2>/dev/null \
      && git config user.email gate-run-check@example.com \
      && git config user.name gate-run-check \
      && git config commit.gpgsign false \
      && echo base > file.txt \
      && printf '.gate-run/\n' > .gitignore \
      && git add -A \
      && git commit -q -m init
  ) || { echo "FATAL: could not build fixture repo $fix"; exit 1; }
}

# Run the REAL driver inside the fixture with the shims first on PATH.
run_gate() {
  ( cd "$fix" && GRC_LOG="$LOG" GRC_RC_DIR="$RC_DIR" PATH="$work/bin:$PATH" bash "$GATE" "$@" )
}
reset_log()  { : > "$LOG"; }
log_lines()  { wc -l < "$LOG" | tr -d ' '; }
state_file() { echo "$fix/.gate-run/state.json"; }

# =============================================================================
# Case A: happy path — all steps stubbed green -> exit 0, eight steps done.
# The expected list is HARDCODED, never derived from gate-run.sh: deriving it from the
# script under test would make this case unable to see the drift it exists to catch.
# =============================================================================
new_fixture A
reset_log
run_gate >/dev/null 2>&1; rc=$?
if [ "$rc" -ne 0 ]; then
  fail A "all-green run exited $rc (expected 0)"
elif [ "$(log_lines)" != "8" ]; then
  fail A "expected 8 step invocations, got $(log_lines): $(cat "$LOG")"
elif [ "$(sed -n '1p' "$LOG")" != "make docs" ] \
  || [ "$(sed -n '2p' "$LOG")" != "cargo test" ] \
  || [ "$(sed -n '3p' "$LOG")" != "cargo test -p ls-core" ] \
  || [ "$(sed -n '4p' "$LOG")" != "make docs-check" ] \
  || [ "$(sed -n '5p' "$LOG")" != "make lane-check" ] \
  || [ "$(sed -n '6p' "$LOG")" != "make adapter-check" ] \
  || [ "$(sed -n '7p' "$LOG")" != "make script-check" ] \
  || [ "$(sed -n '8p' "$LOG")" != "make todo-check" ]; then
  fail A "step order wrong: $(cat "$LOG")"
elif [ "$(grep -c '"status":"done"' "$(state_file)")" != "8" ]; then
  fail A "state.json does not record eight done steps"
elif git -C "$fix" status --porcelain | grep -q '\.gate-run'; then
  fail A ".gate-run/ leaked into git status in the fixture"
else
  ok A "happy path: exit 0, eight steps in AGENTS.md order, eight done in state, .gate-run ignored"
fi

# --- Case A2: re-run with unchanged tree -> nothing re-runs, exit 0 -----------
reset_log
run_gate >/dev/null 2>&1; rc=$?
if [ "$rc" -ne 0 ]; then
  fail A2 "no-op re-run exited $rc (expected 0)"
elif [ "$(log_lines)" != "0" ]; then
  fail A2 "no-op re-run invoked steps: $(cat "$LOG")"
else
  ok A2 "unchanged tree + all done: re-run invokes nothing, exit 0"
fi

# =============================================================================
# Case B (AE6): step 3 fails -> stop, record failure, exit with step's code;
# resume re-runs from step 3 WITHOUT re-running steps 1-2.
# =============================================================================
new_fixture B
echo 7 > "$RC_DIR/cargo_test_-p_ls-core"
reset_log
run_gate >/dev/null 2>&1; rc=$?
if [ "$rc" -ne 7 ]; then
  fail B1 "failing step exit code not propagated: got $rc (expected 7)"
elif [ "$(log_lines)" != "3" ]; then
  fail B1 "driver did not stop at the failing step: $(cat "$LOG")"
elif ! grep -q '"n":3,.*"status":"failed"' "$(state_file)"; then
  fail B1 "state.json does not record step 3 as failed"
elif [ "$(grep -c '"status":"done"' "$(state_file)")" != "2" ]; then
  fail B1 "state.json does not record steps 1-2 as done"
else
  ok B1 "stubbed failure at step 3: driver stops, records failed, exits 7"
fi

rm -f "$RC_DIR/cargo_test_-p_ls-core"
reset_log
run_gate >/dev/null 2>&1; rc=$?
if [ "$rc" -ne 0 ]; then
  fail B2 "resume after fix exited $rc (expected 0)"
elif [ "$(log_lines)" != "6" ]; then
  fail B2 "resume ran $(log_lines) steps (expected 6): $(cat "$LOG")"
elif [ "$(sed -n '1p' "$LOG")" != "cargo test -p ls-core" ]; then
  fail B2 "resume did not start at step 3: $(cat "$LOG")"
elif grep -q '^make docs$' "$LOG"; then
  fail B2 "resume re-ran step 1 (steps 1-2 were still valid): $(cat "$LOG")"
else
  ok B2 "resume starts at failed step 3; valid steps 1-2 NOT re-run"
fi

# --- Case C (AE6): tracked-file change invalidates recorded steps -------------
echo change >> "$fix/file.txt"
reset_log
run_gate >/dev/null 2>&1; rc=$?
if [ "$rc" -ne 0 ] || [ "$(log_lines)" != "8" ]; then
  fail C "tracked-file edit: expected full 8-step re-run rc=0, got rc=$rc steps=$(log_lines)"
elif [ "$(sed -n '1p' "$LOG")" != "make docs" ]; then
  fail C "invalidated run did not restart at step 1: $(cat "$LOG")"
else
  ok C "tracked-file edit changes fingerprint: all recorded steps invalidated and re-run"
fi

# =============================================================================
# Case D (KTD4 whole-repo arm): untracked-file coverage.
# =============================================================================
# D1: a NEW untracked file appears after a green run -> invalidates.
echo note > "$fix/note.txt"
reset_log
run_gate >/dev/null 2>&1; rc=$?
if [ "$rc" -ne 0 ] || [ "$(log_lines)" != "8" ]; then
  fail D1 "new untracked file: expected full re-run rc=0, got rc=$rc steps=$(log_lines)"
else
  ok D1 "new untracked file invalidates and re-runs the recorded steps"
fi

# D2: EDIT the already-untracked file (porcelain output identical; only the
# per-file content digest can see this) -> invalidates.
echo more >> "$fix/note.txt"
reset_log
run_gate >/dev/null 2>&1; rc=$?
if [ "$rc" -ne 0 ] || [ "$(log_lines)" != "8" ]; then
  fail D2 "edited untracked file: expected full re-run rc=0, got rc=$rc steps=$(log_lines)"
else
  ok D2 "content edit of an already-untracked file invalidates (content-digest arm)"
fi

# =============================================================================
# Case F: live concurrency — second invocation refuses while a run holds the lock.
# =============================================================================
new_fixture F
echo block > "$RC_DIR/make_docs"
reset_log
run_gate >/dev/null 2>&1 &
bg=$!
i=0
while [ ! -d "$fix/.gate-run/lock" ] && [ "$i" -lt 100 ]; do sleep 0.1; i=$((i+1)); done
if [ ! -d "$fix/.gate-run/lock" ]; then
  fail F1 "background run never took the lock"
  kill "$bg" 2>/dev/null
else
  out="$(run_gate 2>&1)"; rc=$?
  if [ "$rc" -eq 0 ]; then
    fail F1 "second invocation exited 0 while a run was live (expected refusal)"
  elif ! printf '%s' "$out" | grep -qi 'pid'; then
    fail F1 "refusal message does not name the holder pid: $out"
  elif [ ! -d "$fix/.gate-run/lock" ]; then
    fail F1 "second invocation removed a lock it did not acquire"
  else
    ok F1 "concurrent invocation refused non-zero (rc=$rc) with holder pid; foreign lock preserved"
  fi
fi
touch "$RC_DIR/unblock"
wait "$bg"; bg_rc=$?
if [ "$bg_rc" -ne 0 ]; then
  fail F2 "background run finished non-zero ($bg_rc) after unblock"
elif [ -d "$fix/.gate-run/lock" ]; then
  fail F2 "lock not released after the run finished"
else
  ok F2 "background run completed green and released the lock"
fi
rm -f "$RC_DIR/make_docs" "$RC_DIR/unblock"

# =============================================================================
# Case F3: signal safety — SIGTERM a live run: the driver must TERMINATE (no
# further steps) AND release the lock. (A combined EXIT/INT/TERM cleanup trap
# would free the lock while the driver kept running remaining steps.)
# =============================================================================
new_fixture F3
echo block > "$RC_DIR/make_docs"
reset_log
run_gate >/dev/null 2>&1 &
bg=$!
i=0
while ! grep -q '^make docs$' "$LOG" 2>/dev/null && [ "$i" -lt 100 ]; do sleep 0.1; i=$((i+1)); done
gpid="$(cat "$fix/.gate-run/lock/pid" 2>/dev/null || echo '')"
if ! grep -q '^make docs$' "$LOG" 2>/dev/null || [ -z "$gpid" ]; then
  fail F3 "live run never started step 1 / never wrote the lock pid"
  touch "$RC_DIR/unblock"
  wait "$bg" 2>/dev/null
else
  kill -TERM "$gpid" 2>/dev/null
  sleep 0.3                    # let the signal land while step 1 is still blocked
  touch "$RC_DIR/unblock"      # step 1 returns; the pending TERM trap must now fire
  wait "$bg"; rc=$?
  if [ "$rc" -ne 143 ]; then
    fail F3 "SIGTERM'd run exited $rc (expected 143)"
  elif [ "$(log_lines)" != "1" ]; then
    fail F3 "SIGTERM'd driver ran further steps after the signal: $(cat "$LOG")"
  elif [ -d "$fix/.gate-run/lock" ]; then
    fail F3 "SIGTERM'd run did not release the lock"
  else
    ok F3 "SIGTERM: driver exits 143 promptly, runs no further steps, lock released"
  fi
fi
rm -f "$RC_DIR/make_docs" "$RC_DIR/unblock"

# =============================================================================
# Case G: --status — parseable, names the next step, never runs steps.
# =============================================================================
new_fixture G
reset_log
out="$(run_gate --status)"; rc=$?
if [ "$rc" -ne 0 ]; then
  fail G1 "--status on fresh repo exited $rc"
elif [ "$(log_lines)" != "0" ]; then
  fail G1 "--status invoked gate steps: $(cat "$LOG")"
elif ! printf '%s\n' "$out" | grep -q '^step=1 name=docs status=pending fingerprint=-$'; then
  fail G1 "fresh --status step line malformed: $out"
elif ! printf '%s\n' "$out" | grep -q '^next=docs$'; then
  fail G1 "fresh --status next line malformed: $out"
elif [ "$(printf '%s\n' "$out" | grep -c '^step=')" != "8" ]; then
  fail G1 "--status did not print eight step lines: $out"
else
  ok G1 "--status on fresh repo: eight pending step lines, next=docs, no steps run"
fi

echo 5 > "$RC_DIR/cargo_test_-p_ls-core"
run_gate >/dev/null 2>&1
rm -f "$RC_DIR/cargo_test_-p_ls-core"
out="$(run_gate --status)"; rc=$?
if [ "$rc" -ne 0 ]; then
  fail G2 "--status after failure exited $rc"
elif ! printf '%s\n' "$out" | grep -q '^step=2 name=cargo-test status=done fingerprint=[0-9a-f]\{64\}$'; then
  fail G2 "--status does not show step 2 done with a hex fingerprint: $out"
elif ! printf '%s\n' "$out" | grep -q '^step=3 name=cargo-test-ls-core status=failed '; then
  fail G2 "--status does not show step 3 failed: $out"
elif ! printf '%s\n' "$out" | grep -q '^next=cargo-test-ls-core$'; then
  fail G2 "--status next does not name the failed step: $out"
else
  ok G2 "--status after failure: done/failed per step, next names the resume step"
fi

run_gate >/dev/null 2>&1
out="$(run_gate --status)"
if [ "$(printf '%s\n' "$out" | grep -c ' status=done ')" != "8" ] \
  || ! printf '%s\n' "$out" | grep -q '^next=none$'; then
  fail G3 "--status after green run should show eight done + next=none: $out"
else
  ok G3 "--status after green run: eight done, next=none"
fi

# =============================================================================
# Case H: env scrub — steps must NOT inherit the operator shell's LS_* env
# (documented false-red: a stray LS_TURN_EXPECT_VERSION reddens lab tests).
# The shims append to $GRC_RC_DIR/canary-leak if they see LS_PROBE_CANARY.
# =============================================================================
new_fixture H
reset_log
rm -f "$RC_DIR/canary-leak"
( export LS_PROBE_CANARY=leak-me; run_gate >/dev/null 2>&1 ); rc=$?
if [ "$rc" -ne 0 ]; then
  fail H "run with LS_PROBE_CANARY exported exited $rc (expected 0)"
elif [ -f "$RC_DIR/canary-leak" ]; then
  fail H "steps inherited the operator LS_* env: $(cat "$RC_DIR/canary-leak")"
elif [ "$(log_lines)" != "8" ]; then
  fail H "expected 8 step invocations, got $(log_lines): $(cat "$LOG")"
else
  ok H "operator LS_* env scrubbed from every step's environment"
fi

# =============================================================================
# Case I: fingerprint digest failure must ALWAYS invalidate. A dangling
# symlink among the untracked files kills `git hash-object --stdin-paths`
# partway; a STABLE error constant would leave the fingerprint blind to edits
# of later-sorting untracked files (false green — recorded steps stay done).
# =============================================================================
new_fixture I
run_gate >/dev/null 2>&1                     # green baseline
ln -s does-not-exist "$fix/a-dangling-link"  # sorts BEFORE z-later.txt
echo v1 > "$fix/z-later.txt"
run_gate >/dev/null 2>&1                     # records fingerprints under digest failure
echo v2 >> "$fix/z-later.txt"                # edit a LATER-sorting untracked file
reset_log
run_gate >/dev/null 2>&1; rc=$?
if [ "$rc" -ne 0 ]; then
  fail I "run after edit behind a dangling symlink exited $rc (expected 0)"
elif [ "$(log_lines)" != "8" ]; then
  fail I "digest failure did not invalidate: expected full 8-step re-run, got $(log_lines) (false green)"
else
  ok I "unreadable untracked path: digest failure invalidates recorded steps (re-run, never false green)"
fi

# =============================================================================
if [ "$fails" -ne 0 ]; then
  echo "gate-run-check: FAILED"; exit 1
fi
echo "gate-run-check: all driver cases pass"
