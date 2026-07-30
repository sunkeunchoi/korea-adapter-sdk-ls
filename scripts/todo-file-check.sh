#!/usr/bin/env bash
# Legacy TODO-file guard (plan 2026-07-29-002, U8; requirement R16, decision KTD5).
#
# The queue cutover (U7) retires the legacy TODO staging files — TODO.ATTENDED.md,
# TODO.OFFLINE.md, and the dated **/TODO-*.md convention — in favor of
# queue/items.jsonl as the sole staging location. This guard makes that
# retirement mechanical, with KTD5 polarity:
#
#   Shadow phase   — queue/cutover-verdict.json ABSENT, or present WITHOUT a PASS
#                    verdict -> exit 0 regardless of TODO files (the guard is
#                    inert until the U7 cutover records its verdict).
#   Enforced phase — verdict PASS -> any legacy TODO file (TODO.ATTENDED.md,
#                    TODO.OFFLINE.md, or **/TODO-*.md, excluding docs/ and
#                    target/ at any depth) FAILS loud, naming the offending paths.
#
# The verdict is read with a tolerant STRING SCAN, never serde/jq (mirrors
# merge_block.rs's manual scan): strip ALL whitespace, look for
# "verdict":"PASS" — so both `"verdict": "PASS"` and `"verdict":"PASS"` count.
#
# Search scope: `git ls-files --cached --others --exclude-standard` (tracked +
# untracked-but-not-ignored), so gitignored junk (data/, state/, probes/,
# .gate-run/, target/, ...) can never trip it.
#
# The Rust twin lives in adapters/nautilus/lab/tests/todo_merge_block.rs (the
# tree-state coupling test, #[ignore]-free in `make adapter-check`); it also runs
# this script's --self-test so the two stay coupled.
#
# Usage:
#   scripts/todo-file-check.sh               check this repo (what `make todo-check` runs)
#   scripts/todo-file-check.sh --root DIR    check the repo at DIR (used by --self-test)
#   scripts/todo-file-check.sh --self-test   run the mktemp fixture-repo scenarios
set -uo pipefail

script_path="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"

# Tolerant verdict scan (KTD5). $1 = repo root. True iff the verdict file exists
# and, with all whitespace stripped, contains "verdict":"PASS".
verdict_is_pass() {
  local f="$1/queue/cutover-verdict.json"
  [ -f "$f" ] || return 1
  tr -d '[:space:]' < "$f" | grep -q '"verdict":"PASS"'
}

# Legacy TODO files in scope. $1 = repo root; offending repo-relative paths on
# stdout (sorted). Tracked + untracked-not-ignored, minus docs/ and target/.
find_offenders() {
  git -C "$1" ls-files --cached --others --exclude-standard 2>/dev/null \
    | grep -E '(^|/)(TODO\.ATTENDED\.md|TODO\.OFFLINE\.md|TODO-[^/]*\.md)$' \
    | grep -Ev '(^|/)docs/' \
    | grep -Ev '(^|/)target/' \
    | LC_ALL=C sort
  return 0
}

# The guard itself. $1 = repo root. Exit 0 = green, 1 = violation.
check_root() {
  local root="$1" offenders
  if ! verdict_is_pass "$root"; then
    echo "ok[shadow]: no cutover verdict — TODO-file guard inert"
    return 0
  fi
  offenders="$(find_offenders "$root")"
  if [ -z "$offenders" ]; then
    echo "ok[enforced]: verdict PASS and no legacy TODO files"
    return 0
  fi
  echo "FAIL[enforced]: cutover verdict is PASS but legacy TODO staging files remain (queue/items.jsonl is the sole staging location):"
  printf '%s\n' "$offenders" | sed 's/^/  /'
  return 1
}

# --- Self-test: run the REAL script against mktemp git fixture repos ----------
# (real-recipe-shim pattern — no re-implemented guard logic to drift).
self_test() {
  local work fails=0 d out rc
  work="$(mktemp -d)"
  # Expand $work NOW: it is local to this function and would be unbound (set -u)
  # by the time the EXIT trap fires at script exit.
  trap "rm -rf '$work'" EXIT

  new_fixture() { # $1 = name -> fixture dir on stdout
    local f="$work/$1"
    mkdir -p "$f"
    git -C "$f" init -q
    printf '%s\n' "$f"
  }

  # --- Case A: Shadow (no verdict file) + legacy TODO files present -> OK ------
  d="$(new_fixture a-shadow)"
  printf 'legacy\n' > "$d/TODO.ATTENDED.md"
  printf 'legacy\n' > "$d/TODO.OFFLINE.md"
  mkdir -p "$d/adapters/nautilus/lab"
  printf 'legacy\n' > "$d/adapters/nautilus/lab/TODO-2026-07-28-A-x.md"
  out="$(bash "$script_path" --root "$d")"; rc=$?
  if [ "$rc" -ne 0 ] || ! printf '%s' "$out" | grep -q 'ok\[shadow\]'; then
    echo "FAIL[A]: shadow phase (no verdict) with TODO files present must be inert-OK"; echo "$out"; fails=1
  else
    echo "ok[A]: shadow phase (no verdict) stays green with legacy TODO files present"
  fi

  # --- Case B: verdict present but NOT PASS (HOLD) + TODO files -> still inert -
  d="$(new_fixture b-hold)"
  mkdir -p "$d/queue"
  printf '{\n  "verdict": "HOLD"\n}\n' > "$d/queue/cutover-verdict.json"
  printf 'legacy\n' > "$d/TODO.ATTENDED.md"
  out="$(bash "$script_path" --root "$d")"; rc=$?
  if [ "$rc" -ne 0 ] || ! printf '%s' "$out" | grep -q 'ok\[shadow\]'; then
    echo "FAIL[B]: a non-PASS verdict must leave the guard inert"; echo "$out"; fails=1
  else
    echo "ok[B]: non-PASS (HOLD) verdict leaves the guard inert"
  fi

  # --- Case C: verdict PASS (whitespaced form) + no legacy files -> OK ----------
  d="$(new_fixture c-clean)"
  mkdir -p "$d/queue"
  printf '{\n  "verdict": "PASS"\n}\n' > "$d/queue/cutover-verdict.json"
  out="$(bash "$script_path" --root "$d")"; rc=$?
  if [ "$rc" -ne 0 ] || ! printf '%s' "$out" | grep -q 'ok\[enforced\]'; then
    echo "FAIL[C]: verdict PASS with a clean tree must pass enforced"; echo "$out"; fails=1
  else
    echo "ok[C]: verdict PASS (whitespaced JSON) with no legacy TODO files passes"
  fi

  # --- Case D: verdict PASS (compact form) + planted TODO-*.md -> FAIL w/ path --
  d="$(new_fixture d-planted)"
  mkdir -p "$d/queue" "$d/adapters/nautilus/lab"
  printf '{"verdict":"PASS"}\n' > "$d/queue/cutover-verdict.json"
  printf 'planted\n' > "$d/adapters/nautilus/lab/TODO-2026-01-01-X.md"
  out="$(bash "$script_path" --root "$d")"; rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "FAIL[D]: verdict PASS with a planted TODO-*.md exited 0 (expected non-zero)"; echo "$out"; fails=1
  elif ! printf '%s' "$out" | grep -q 'adapters/nautilus/lab/TODO-2026-01-01-X.md'; then
    echo "FAIL[D]: non-zero exit but the offending path was not named"; echo "$out"; fails=1
  else
    echo "ok[D]: verdict PASS (compact JSON) with a planted TODO-*.md fails naming the path"
  fi

  # --- Case E: verdict PASS; TODO files ONLY in docs/, target/, gitignored -> OK
  d="$(new_fixture e-excluded)"
  mkdir -p "$d/queue" "$d/docs/plans" "$d/target/debug" "$d/junk"
  printf '{"verdict": "PASS"}\n' > "$d/queue/cutover-verdict.json"
  printf 'plan doc\n' > "$d/docs/plans/TODO-2026-01-01-Y.md"
  printf 'build junk\n' > "$d/target/debug/TODO-2026-01-01-Z.md"
  printf 'junk/\n' > "$d/.gitignore"
  printf 'ignored\n' > "$d/junk/TODO-2026-01-01-W.md"
  out="$(bash "$script_path" --root "$d")"; rc=$?
  if [ "$rc" -ne 0 ] || ! printf '%s' "$out" | grep -q 'ok\[enforced\]'; then
    echo "FAIL[E]: docs/, target/, and gitignored TODO files must not trip the guard"; echo "$out"; fails=1
  else
    echo "ok[E]: docs/, target/, and gitignored TODO files are excluded"
  fi

  # --- Case F: verdict PASS; TODO file in a NESTED docs/ dir -> OK (excluded) --
  d="$(new_fixture f-nested-docs)"
  mkdir -p "$d/queue" "$d/sub/docs"
  printf '{"verdict":"PASS"}\n' > "$d/queue/cutover-verdict.json"
  printf 'nested doc\n' > "$d/sub/docs/TODO-2026-01-01-X.md"
  out="$(bash "$script_path" --root "$d")"; rc=$?
  if [ "$rc" -ne 0 ] || ! printf '%s' "$out" | grep -q 'ok\[enforced\]'; then
    echo "FAIL[F]: a TODO file under a nested docs/ dir must not trip the guard"; echo "$out"; fails=1
  else
    echo "ok[F]: a TODO file under a nested docs/ dir is excluded"
  fi

  if [ "$fails" -ne 0 ]; then
    echo "todo-file-check --self-test: FAILED"; return 1
  fi
  echo "todo-file-check --self-test: all guard cases pass"
  return 0
}

case "${1:-}" in
  "")          check_root "$repo_root"; exit $? ;;
  --root)      [ -n "${2:-}" ] || { echo "usage: scripts/todo-file-check.sh [--root DIR | --self-test]" >&2; exit 64; }
               check_root "$2"; exit $? ;;
  --self-test) self_test; exit $? ;;
  *)           echo "usage: scripts/todo-file-check.sh [--root DIR | --self-test]" >&2; exit 64 ;;
esac
