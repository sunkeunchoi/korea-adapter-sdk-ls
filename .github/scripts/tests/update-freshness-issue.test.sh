#!/usr/bin/env bash
#
# Unit tests for update-freshness-issue.sh — the dry-run decision core. No `gh`
# is ever invoked: the script's pure helpers are sourced directly and issue state
# is mocked. Run with: bash .github/scripts/tests/update-freshness-issue.test.sh
#
# Each test asserts the chosen ACTION (or a helper's output) against the cadence
# plan's scenarios, mapped to their acceptance examples (AE*).

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$HERE/../update-freshness-issue.sh"

# shellcheck source=../update-freshness-issue.sh
source "$SCRIPT"

pass=0
fail=0

ok() { printf 'ok   - %s\n' "$1"; pass=$((pass + 1)); }
no() { printf 'FAIL - %s\n     expected: %s\n     actual:   %s\n' "$1" "$2" "$3"; fail=$((fail + 1)); }

assert_eq() { # desc expected actual
  if [ "$2" = "$3" ]; then ok "$1"; else no "$1" "$2" "$3"; fi
}

# Write a findings.json fixture with the given stale TR codes (comma-separated,
# or empty for all-fresh) and echo the path.
fixture() { # "t1102,token" -> path
  local codes="$1" f
  f=$(mktemp)
  local entries=""
  if [ -n "$codes" ]; then
    local IFS=','
    for c in $codes; do
      entries+="{\"tr_code\":\"$c\",\"last_reviewed\":\"2026-03-01\",\"age_days\":111,\"severity\":\"evidence\"},"
    done
    entries="${entries%,}"
  fi
  cat >"$f" <<EOF
{"as_of":"2026-06-20","window_days":90,"recommended_count":6,"has_errors":false,"stale":[$entries],"unparseable":[]}
EOF
  echo "$f"
}

# Run the script (not sourced) with --dry-run and echo just the ACTION token.
run_action() { # findings state body number
  FRESHNESS_DRYRUN_ISSUE_STATE="$2" \
  FRESHNESS_DRYRUN_ISSUE_BODY="$3" \
  FRESHNESS_DRYRUN_ISSUE_NUMBER="${4:-0}" \
  FRESHNESS_MAINTAINER="@maintainer" \
    bash "$SCRIPT" --dry-run "$1" | sed -n 's/^ACTION: //p'
}

marker() { echo "<!-- freshness-stale: $1 -->"; }

# --- AE1: stale, no existing issue → create + notify ------------------------
f=$(fixture "t1102,token")
assert_eq "AE1 stale + no issue → create+notify" "create+notify" "$(run_action "$f" none "" 0)"

# Body lists both stale TRs and embeds the marker (render_body is pure).
body=$(render_body "$f" "$(compute_stale_set "$f")")
case "$body" in
  *t1102*token*|*token*t1102*) ok "AE1 body lists both stale TRs" ;;
  *) no "AE1 body lists both stale TRs" "contains t1102 and token" "$body" ;;
esac
case "$body" in
  *"freshness-stale:"*) ok "AE1 body embeds the marker block" ;;
  *) no "AE1 body embeds the marker block" "contains marker" "$body" ;;
esac

# --- AE2: all fresh, an open issue exists → close + all-clear ---------------
f=$(fixture "")
assert_eq "AE2 fresh + open issue → close" "close" "$(run_action "$f" open "$(marker 't1102')" 7)"
# All fresh, no issue → noop (no spurious create).
assert_eq "AE2 fresh + no issue → noop" "noop" "$(run_action "$f" none "" 0)"

# --- AE3: stale, open issue, SAME stale set → edit only (no spam) -----------
f=$(fixture "t1102")
assert_eq "AE3 stale + same set → edit (silent)" "edit" "$(run_action "$f" open "$(marker 't1102')" 7)"

# Idempotency: first run (no issue) creates, second (now open, same set) edits —
# never two creates.
f2=$(fixture "t1102")
a1=$(run_action "$f2" none "" 0)
a2=$(run_action "$f2" open "$(marker 't1102')" 7)
assert_eq "AE3 idempotent: run 1 creates" "create+notify" "$a1"
assert_eq "AE3 idempotent: run 2 edits, not creates" "edit" "$a2"

# --- AE9: stale, issue was CLOSED (manual close) → reopen + notify ----------
f=$(fixture "t1102")
assert_eq "AE9 stale + closed → reopen+notify" "reopen+notify" "$(run_action "$f" closed "$(marker 't1102')" 7)"

# --- AE11: stale {t1102} open, now {t1102,t8412} → edit + notify ------------
f=$(fixture "t1102,t8412")
assert_eq "AE11 newly-stale TR → edit+notify" "edit+notify" "$(run_action "$f" open "$(marker 't1102')" 7)"

# --- Diff rule: shrinking-but-still-stale → edit only (not notify, not close) -
f=$(fixture "t1102")
assert_eq "shrinking set {t1102,t8412}→{t1102} → edit" "edit" "$(run_action "$f" open "$(marker 't1102,t8412')" 7)"

# --- Missing/garbled marker → prior empty → edit+notify, never error --------
f=$(fixture "t1102")
assert_eq "missing marker → newly-stale → edit+notify" "edit+notify" "$(run_action "$f" open "no marker here" 7)"
assert_eq "garbled marker → newly-stale → edit+notify" "edit+notify" "$(run_action "$f" open "<!-- freshness-stale: garbled" 7)"
# parse_prior_stale must not error on a bad marker (would trip R9).
if parse_prior_stale "totally unparseable body" >/dev/null 2>&1; then
  ok "parse_prior_stale tolerates a bad marker"
else
  no "parse_prior_stale tolerates a bad marker" "exit 0" "non-zero"
fi

# --- AE5: generated body carries NO Work Queue prefix/labels ----------------
f=$(fixture "t1102,token")
body=$(render_body "$f" "$(compute_stale_set "$f")")
case "$body" in
  *"[SDK work item]"*|*"queue:"*|*"source:"*|*"class:"*|*"support:"*|*"gate:"*)
    no "AE5 body avoids Work Queue taxonomy" "no queue labels/prefix" "$body" ;;
  *) ok "AE5 body avoids Work Queue taxonomy" ;;
esac

# --- decide_action core: direct unit coverage of every branch ---------------
assert_eq "core: fresh+none → noop"          "noop"          "$(decide_action "" none "")"
assert_eq "core: fresh+open → close"         "close"         "$(decide_action "" open "t1102")"
assert_eq "core: stale+none → create+notify" "create+notify" "$(decide_action "t1102" none "")"
assert_eq "core: stale+closed → reopen"      "reopen+notify" "$(decide_action "t1102" closed "t1102")"
assert_eq "core: stale+open same → edit"     "edit"          "$(decide_action $'t1102' open $'t1102')"
assert_eq "core: stale+open newly → notify"  "edit+notify"   "$(decide_action $'t1102\nt8412' open $'t1102')"

# --- live path (real script, stubbed gh) -----------------------------------
# These exercise the gh I/O path the dry-run tests never touch — the seam where
# the issue body round-trips (render → resolve → parse) and where branch exit
# codes actually reach the workflow. Without them, a subshell-scoped body or a
# `[ ] && cmd` silent-branch exit-1 ships green.

# Build a fake `gh` on PATH: echoes canned `issue list` JSON, logs every call,
# prints a URL for `issue create`. Stored under $1/bin/gh.
make_fake_gh() {
  mkdir -p "$1/bin"
  cat >"$1/bin/gh" <<'GH'
#!/usr/bin/env bash
echo "gh $*" >> "$GH_FAKE_LOG"
if [ "$1" = "issue" ] && [ "$2" = "list" ]; then
  cat "$GH_FAKE_LIST"
elif [ "$1" = "issue" ] && [ "$2" = "create" ]; then
  echo "https://github.com/o/r/issues/42"
fi
exit 0
GH
  chmod +x "$1/bin/gh"
}

# run_live FINDINGS LIST_JSON [MAINTAINER] → sets LIVE_RC, LIVE_LOG
run_live() {
  local f="$1" listjson="$2" maint="${3:-@maintainer}" tmp listfile
  tmp=$(mktemp -d)
  make_fake_gh "$tmp"
  listfile="$tmp/list.json"
  printf '%s' "$listjson" >"$listfile"
  GH_FAKE_LOG="$tmp/log" GH_FAKE_LIST="$listfile" FRESHNESS_MAINTAINER="$maint" \
    PATH="$tmp/bin:$PATH" bash "$SCRIPT" "$f" >/dev/null 2>&1
  LIVE_RC=$?
  LIVE_LOG=$(cat "$tmp/log" 2>/dev/null || true)
}

# Silent edit: open issue whose marker matches the current stale set. MUST exit 0
# and MUST NOT comment (guards P0 subshell-body bug AND P0 silent-branch exit-1).
f=$(fixture "t1102")
run_live "$f" '[{"number":7,"state":"OPEN","body":"<!-- freshness-stale: t1102 -->"}]'
assert_eq "live: silent edit exits 0" "0" "$LIVE_RC"
case "$LIVE_LOG" in
  *"issue edit"*) ok "live: silent edit calls issue edit" ;;
  *) no "live: silent edit calls issue edit" "log has 'issue edit'" "$LIVE_LOG" ;;
esac
case "$LIVE_LOG" in
  *"issue comment"*) no "live: silent edit does NOT notify" "no 'issue comment'" "$LIVE_LOG" ;;
  *) ok "live: silent edit does NOT notify (no spam)" ;;
esac

# Newly-stale TR: open issue marker is a subset of the current stale set → notify.
f=$(fixture "t1102,t8412")
run_live "$f" '[{"number":7,"state":"OPEN","body":"<!-- freshness-stale: t1102 -->"}]'
assert_eq "live: edit+notify exits 0" "0" "$LIVE_RC"
case "$LIVE_LOG" in
  *"issue comment"*) ok "live: newly-stale TR notifies" ;;
  *) no "live: newly-stale TR notifies" "log has 'issue comment'" "$LIVE_LOG" ;;
esac

# Create: no existing issue → create + notify.
f=$(fixture "t1102")
run_live "$f" '[]'
assert_eq "live: create exits 0" "0" "$LIVE_RC"
case "$LIVE_LOG" in
  *"issue create"*"issue comment"*) ok "live: create then notify" ;;
  *) no "live: create then notify" "create + comment" "$LIVE_LOG" ;;
esac

# Handle sanitization: a malformed FRESHNESS_MAINTAINER must not reach the comment.
f=$(fixture "t1102")
run_live "$f" '[]' '@evil </b> @everyone'
case "$LIVE_LOG" in
  *"evil"*|*"everyone"*) no "live: malformed handle is dropped" "no injected handle" "$LIVE_LOG" ;;
  *) ok "live: malformed handle is dropped from notification" ;;
esac

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
