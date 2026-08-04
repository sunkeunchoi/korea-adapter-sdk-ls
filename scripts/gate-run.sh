#!/usr/bin/env bash
# gate-run.sh — resumable driver for the AGENTS.md offline gate
# (plan 2026-07-29-002, U4; decision KTD4; requirements R10/R11).
#
# Runs the gate steps IN ORDER, recording per-step state plus a whole-tree
# fingerprint to the gitignored state file .gate-run/state.json (atomic
# tmp+rename):
#   1. docs                make docs
#   2. cargo-test          cargo test               (root workspace)
#   3. cargo-test-ls-core  cargo test -p ls-core
#   4. docs-check          make docs-check
#   5. lane-check          make lane-check
#   6. adapter-check       make adapter-check
#   7. script-check        make script-check
#   8. todo-check          make todo-check
#
# script-check sits AFTER adapter-check on purpose: it replays the marshalled
# argv against adapters/nautilus/target/debug/calendar-fetch-inputs, and
# adapter-check is the step that builds it. There is no earlier position it
# could occupy, so no fail-fast ordering is being traded away.
#
# Resume: invoked with no args it recomputes the tree fingerprint, compares it
# against each completed step's recorded-at-completion fingerprint, and re-runs
# from the first step that never completed or whose fingerprint mismatches —
# all steps from that point re-run. A mismatch can only produce a spurious
# re-run, never a false green. A step killed mid-run is left status=running,
# which counts as incomplete (a root `cargo test` cannot suspend; resumability
# means knowing it must restart).
#
# Fingerprint (KTD4): SHA-256 over `git rev-parse HEAD`, the porcelain status
# (`git status --porcelain -z`), staged/unstaged diff digests, and per-file
# content digests of untracked files (`git ls-files --others
# --exclude-standard`) — whole-repo coverage, including new or edited files
# that no diff records. .gate-run/ is excluded by construction: it is
# gitignored, and the untracked enumeration excludes ignored paths.
#
# Usage:
#   scripts/gate-run.sh            run / resume the gate (from anywhere inside the repo)
#   scripts/gate-run.sh --status   print machine-readable state; NEVER runs steps
#
# --status output (STABLE contract — the lab-next sequence probe parses it):
#   one line per step:  step=<n> name=<name> status=done|failed|pending fingerprint=<hex64|->
#   final line:         next=<name|none>
# `done` means the step completed successfully, its recorded fingerprint
# matches the CURRENT tree, and every earlier step is also done; anything a
# run would re-execute reports pending (or failed, if that is what was
# recorded). `fingerprint` is the recorded-at-completion value (`-` if none).
# `next` is the first step a run would execute now; `none` = gate green.
#
# state.json schema (version 1; one step object per line, machine-generated —
# this script is the only writer and reader):
#   {"version":1,"steps":[
#   {"n":<1..8>,"name":"<name>","cmd":"<command>","status":"done|failed|running|pending",
#    "started_at":"<utc|->","ended_at":"<utc|->","exit_code":"<int|->","fingerprint":"<hex64|->"},
#   ... ]}
#
# Locking: `mkdir .gate-run/lock` is the advisory lock (mkdir is atomic);
# holder pid in .gate-run/lock/pid. A second invocation refuses while the lock
# is held and prints the holder pid. Only the acquiring process removes the
# lock (trap on exit); a stale lock after a crash must be removed by hand
# (the refusal message says how).
#
# Exit codes: 0 = gate green (or --status printed); 64 = usage / not a git
# repo; 75 = lock held (another gate run live); otherwise the failing step's
# own exit code.
set -uo pipefail

usage() { echo "usage: scripts/gate-run.sh [--status]" >&2; }

ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"
if [ -z "${ROOT:-}" ]; then
  echo "gate-run: not inside a git repository" >&2
  exit 64
fi
STATE_DIR="$ROOT/.gate-run"
STATE_FILE="$STATE_DIR/state.json"
LOCK_DIR="$STATE_DIR/lock"

STEP_NAMES=(docs cargo-test cargo-test-ls-core docs-check lane-check adapter-check script-check todo-check)
STEP_CMDS=("make docs" "cargo test" "cargo test -p ls-core" "make docs-check" "make lane-check" "make adapter-check" "make script-check" "make todo-check")
NSTEPS=${#STEP_NAMES[@]}

declare -a S_STATUS S_START S_END S_EXIT S_FP EFF
RESUME=0
CURFP=""

sha256() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 | awk '{print $1}'
  else
    sha256sum | awk '{print $1}'
  fi
}

now_utc() { date -u +%Y-%m-%dT%H:%M:%SZ; }

# KTD4 fingerprint: HEAD + porcelain status + staged/unstaged diff digests +
# per-file content digests of untracked files (ignored paths — .gate-run/ —
# excluded by `--exclude-standard`). Untracked names are sorted for
# determinism; `hash-object --stdin-paths` output order matches the name list,
# so hashing names+digests together pins each file's content.
tree_fingerprint() {
  local untracked
  untracked="$(git -C "$ROOT" ls-files --others --exclude-standard 2>/dev/null | LC_ALL=C sort)"
  {
    git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo NO-HEAD
    git -C "$ROOT" status --porcelain -z 2>/dev/null
    printf '\nstaged:%s\n' "$(git -C "$ROOT" diff --cached 2>/dev/null | sha256)"
    printf 'unstaged:%s\n' "$(git -C "$ROOT" diff 2>/dev/null | sha256)"
    printf 'untracked:\n%s\n' "$untracked"
    if [ -n "$untracked" ]; then
      # If hash-object dies partway (e.g. a dangling symlink among the
      # untracked paths), later-sorting files were never digested — a STABLE
      # error constant would then make the fingerprint blind to their edits
      # (false green). Emit a per-invocation unique token instead: digest
      # failure always invalidates (spurious re-run, never a false green).
      printf '%s\n' "$untracked" | git -C "$ROOT" hash-object --stdin-paths 2>/dev/null \
        || echo "HASH-ERR-$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')-$$"
    fi
  } | sha256
}

json_field() { # $1=line $2=field -> value ('' if absent)
  printf '%s' "$1" | sed -n 's/.*"'"$2"'":"\([^"]*\)".*/\1/p'
}

load_state() {
  local i line v
  for ((i = 1; i <= NSTEPS; i++)); do
    S_STATUS[$i]=pending; S_START[$i]="-"; S_END[$i]="-"; S_EXIT[$i]="-"; S_FP[$i]="-"
  done
  [ -f "$STATE_FILE" ] || return 0
  for ((i = 1; i <= NSTEPS; i++)); do
    line="$(grep -F "\"n\":$i," "$STATE_FILE" 2>/dev/null | head -n 1)"
    [ -n "$line" ] || continue
    v="$(json_field "$line" status)";      if [ -n "$v" ]; then S_STATUS[$i]="$v"; fi
    v="$(json_field "$line" started_at)";  if [ -n "$v" ]; then S_START[$i]="$v"; fi
    v="$(json_field "$line" ended_at)";    if [ -n "$v" ]; then S_END[$i]="$v"; fi
    v="$(json_field "$line" exit_code)";   if [ -n "$v" ]; then S_EXIT[$i]="$v"; fi
    v="$(json_field "$line" fingerprint)"; if [ -n "$v" ]; then S_FP[$i]="$v"; fi
  done
}

write_state() { # atomic tmp+rename
  local tmp="$STATE_FILE.tmp-$$" i comma
  mkdir -p "$STATE_DIR"
  {
    printf '{"version":1,"steps":[\n'
    for ((i = 1; i <= NSTEPS; i++)); do
      comma=','
      if [ "$i" -eq "$NSTEPS" ]; then comma=''; fi
      printf '{"n":%d,"name":"%s","cmd":"%s","status":"%s","started_at":"%s","ended_at":"%s","exit_code":"%s","fingerprint":"%s"}%s\n' \
        "$i" "${STEP_NAMES[$((i - 1))]}" "${STEP_CMDS[$((i - 1))]}" \
        "${S_STATUS[$i]}" "${S_START[$i]}" "${S_END[$i]}" "${S_EXIT[$i]}" "${S_FP[$i]}" "$comma"
    done
    printf ']}\n'
  } > "$tmp"
  mv -f "$tmp" "$STATE_FILE"
}

# Sets CURFP, RESUME (first step to run; NSTEPS+1 = all valid) and EFF[1..N]
# (effective status against the CURRENT tree: done|failed|pending).
compute_resume() {
  local i j
  CURFP="$(tree_fingerprint)"
  RESUME=$((NSTEPS + 1))
  for ((i = 1; i <= NSTEPS; i++)); do
    if [ "${S_STATUS[$i]}" = "done" ] && [ "${S_FP[$i]}" = "$CURFP" ]; then
      EFF[$i]="done"
      continue
    fi
    RESUME=$i
    for ((j = i; j <= NSTEPS; j++)); do
      if [ "${S_STATUS[$j]}" = "failed" ]; then EFF[$j]=failed; else EFF[$j]=pending; fi
    done
    break
  done
}

do_status() {
  local i next=none
  load_state
  compute_resume
  for ((i = 1; i <= NSTEPS; i++)); do
    printf 'step=%d name=%s status=%s fingerprint=%s\n' \
      "$i" "${STEP_NAMES[$((i - 1))]}" "${EFF[$i]}" "${S_FP[$i]}"
  done
  if [ "$RESUME" -le "$NSTEPS" ]; then next="${STEP_NAMES[$((RESUME - 1))]}"; fi
  printf 'next=%s\n' "$next"
}

do_run() {
  local i name cmd rc holder envname
  local -a ls_scrub
  mkdir -p "$STATE_DIR"
  if ! mkdir "$LOCK_DIR" 2>/dev/null; then
    holder="unknown"
    if [ -f "$LOCK_DIR/pid" ]; then holder="$(cat "$LOCK_DIR/pid" 2>/dev/null || echo unknown)"; fi
    echo "gate-run: another gate run appears live (lock $LOCK_DIR held, holder pid $holder); refusing." >&2
    echo "gate-run: if that pid is dead this lock is stale — remove it with: rm -rf $LOCK_DIR" >&2
    exit 75
  fi
  echo "$$" > "$LOCK_DIR/pid"
  # Separate signal handlers: clean up AND terminate. A single combined
  # EXIT/INT/TERM trap would free the lock on a pid-targeted signal while the
  # driver kept running remaining steps — lock gone with a run still live (a
  # second gate run could then start; two concurrent root cargo tests is a
  # documented never-do).
  trap 'rm -rf "$LOCK_DIR"' EXIT
  trap 'rm -rf "$LOCK_DIR"; trap - EXIT; exit 130' INT
  trap 'rm -rf "$LOCK_DIR"; trap - EXIT; exit 143' TERM

  if ! git -C "$ROOT" check-ignore -q .gate-run 2>/dev/null; then
    echo "gate-run: WARNING: .gate-run is not gitignored here — its state file will churn the fingerprint (spurious re-runs, never a false green)." >&2
  fi

  load_state
  compute_resume
  if [ "$RESUME" -gt "$NSTEPS" ]; then
    echo "gate-run: all $NSTEPS steps already green for the current tree fingerprint; nothing to do."
    return 0
  fi
  if [ "$RESUME" -gt 1 ]; then
    echo "gate-run: resuming at step $RESUME/$NSTEPS (${STEP_NAMES[$((RESUME - 1))]}); steps 1-$((RESUME - 1)) still valid for the current tree."
  fi
  # Invalidate everything from the resume point on (they all re-run).
  for ((i = RESUME; i <= NSTEPS; i++)); do
    S_STATUS[$i]=pending; S_START[$i]="-"; S_END[$i]="-"; S_EXIT[$i]="-"; S_FP[$i]="-"
  done
  write_state

  # Steps must NOT inherit the operator shell's LS_* env: a stray exported
  # LS_TURN_EXPECT_VERSION reddens lab tests on a pristine tree (documented
  # false-red; see docs/solutions/test-failures/
  # operator-shell-ls-env-makes-the-adapter-suite-look-red-on-pristine-main.md).
  # Build an `env -u NAME ...` scrub list once (bash-3.2-safe: `+=` array
  # append; the `${arr[@]+...}` expansion below tolerates an empty array under
  # `set -u`). Only well-formed variable names are scrubbed (multi-line values
  # can masquerade as names in `env` output).
  ls_scrub=()
  while IFS='=' read -r envname _; do
    case "$envname" in
      LS_*) case "$envname" in *[!A-Za-z0-9_]*) ;; *) ls_scrub+=(-u "$envname") ;; esac ;;
    esac
  done < <(env)

  for ((i = RESUME; i <= NSTEPS; i++)); do
    name="${STEP_NAMES[$((i - 1))]}"
    cmd="${STEP_CMDS[$((i - 1))]}"
    echo "gate-run: step $i/$NSTEPS ($name): $cmd"
    S_STATUS[$i]=running
    S_START[$i]="$(now_utc)"
    write_state
    ( cd "$ROOT" && env ${ls_scrub[@]+"${ls_scrub[@]}"} $cmd )
    rc=$?
    S_END[$i]="$(now_utc)"
    S_EXIT[$i]="$rc"
    S_FP[$i]="$(tree_fingerprint)"
    if [ "$rc" -eq 0 ]; then
      S_STATUS[$i]="done"
      write_state
    else
      S_STATUS[$i]=failed
      write_state
      echo "gate-run: step $i/$NSTEPS ($name) FAILED (exit $rc); state recorded — a re-run resumes here." >&2
      return "$rc"
    fi
  done
  echo "gate-run: gate green — all $NSTEPS steps done; state recorded in $STATE_FILE"
  return 0
}

if [ "$#" -gt 1 ]; then
  usage
  exit 64
fi
case "${1:---run}" in
  --run) do_run; exit $? ;;
  --status) do_status; exit 0 ;;
  *) usage; exit 64 ;;
esac
