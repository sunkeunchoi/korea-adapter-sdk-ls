#!/usr/bin/env bash
#
# update-freshness-issue.sh — idempotent upsert of the single rolling "Evidence
# freshness status" issue from a freshness `--json` report (U1 contract).
#
# This is the repo's first automation helper. It drives one issue to the correct
# state per the cadence plan's decision tree, with three load-bearing invariants:
#
#   * Idempotent — the issue is found by the dedicated `freshness-status` label
#     across `--state all`, so re-runs never create a duplicate (AE3).
#   * Never silent while stale — whenever the report carries stale TRs the issue is
#     ensured open, even after a maintainer manually closed it (reopen + notify),
#     because the bot never trusts the open/closed flag as truth (KTD5/AE9).
#   * Notify on transition only — the body is rewritten every run as a silent
#     dashboard (GitHub does not notify on body edits); a notifying comment that
#     @mentions the maintainer is posted only when staleness is *new* relative to
#     the prior run (fresh→stale, reopen, or a newly-stale TR joining), so same-set
#     re-runs do not spam (KTD4/R10/AE11).
#
# The rolling issue is deliberately NOT a Maintenance Work Queue item: it carries
# the `freshness-status` label and a fixed title, never the `[SDK work item]:`
# prefix or any `queue:*`/`source:*`/`class:*`/`support:*`/`gate:*` label, and
# escalation stays human (R4/AE5, ADR 0013).
#
# The pure decision core (`decide_action`) is separated from all `gh` I/O so the
# gating logic — the actual risk — is unit-testable via `--dry-run` with mocked
# issue state; see tests/update-freshness-issue.bats.
#
# Usage:
#   update-freshness-issue.sh [--dry-run] [FINDINGS_JSON]
#
#   FINDINGS_JSON    path to the U1 `freshness check --json` output (default:
#                    findings.json).
#   --dry-run        print the chosen ACTION line and never call `gh`. Issue state
#                    is read from the mock env vars below instead of the API.
#
# Environment:
#   FRESHNESS_MAINTAINER   handle to @mention on a notifying transition (e.g.
#                          "@octocat"). Sourced from a repo variable by the
#                          workflow, never a hardcoded secret.
#   GH_TOKEN               consumed by `gh` on the live path (set by the workflow).
#
# Dry-run mock inputs (ignored on the live path):
#   FRESHNESS_DRYRUN_ISSUE_STATE    none | open | closed   (default: none)
#   FRESHNESS_DRYRUN_ISSUE_BODY     prior issue body text (for marker parsing)
#   FRESHNESS_DRYRUN_ISSUE_NUMBER   prior issue number (default: 0)

set -euo pipefail

LABEL="freshness-status"
TITLE="Evidence freshness status"
LABEL_COLOR="FBCA04"
LABEL_DESC="Rolling evidence-freshness dashboard (not an SDK work item)"
# Single-line, grep-stable marker embedded in the issue body. The next run parses
# the prior stale set from it to decide notify-vs-silent without ambiguity.
MARKER_PREFIX="<!-- freshness-stale:"
MARKER_SUFFIX="-->"

dry_run=false
findings="findings.json"
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=true ;;
    -*) echo "error: unknown flag $arg" >&2; exit 2 ;;
    *) findings="$arg" ;;
  esac
done

# --- pure helpers ----------------------------------------------------------

# Current stale TR codes from the report, sorted and de-duplicated (one per line).
compute_stale_set() {
  local file="$1"
  jq -r '.stale[].tr_code' "$file" | sort -u
}

# Prior stale TR codes parsed from a previous issue body's marker block. A missing
# or garbled marker yields the empty set — NEVER an error — so the first stale run
# (no marker yet) or a human-edited body reads as "all current TRs are newly
# stale" and notifies once, rather than tripping the workflow's failure path.
parse_prior_stale() {
  local body="$1"
  local line
  line=$(printf '%s\n' "$body" | grep -oE "${MARKER_PREFIX}[^>]*${MARKER_SUFFIX}" | head -n1 || true)
  [ -z "$line" ] && return 0
  # Strip the marker wrapper, then extract code-shaped tokens (alphanumerics and
  # underscores cover every TR code, e.g. t1102, S3_, CSPAQ12200), sorted.
  line="${line#"$MARKER_PREFIX"}"
  line="${line%"$MARKER_SUFFIX"}"
  printf '%s\n' "$line" | grep -oE '[A-Za-z0-9_]+' | sort -u || true
}

# The decision core. Inputs:
#   $1 current stale set (newline-separated, sorted; empty = all fresh)
#   $2 issue state: none | open | closed
#   $3 prior stale set (newline-separated, sorted)
# Echoes exactly one ACTION token:
#   noop | close | create+notify | edit | edit+notify | reopen+notify
decide_action() {
  local current="$1" state="$2" prior="$3"

  if [ -z "$current" ]; then
    case "$state" in
      open) echo "close" ;;   # clear the dashboard with an all-clear comment
      *)    echo "noop" ;;     # nothing stale, nothing open
    esac
    return 0
  fi

  # Stale TRs exist from here on.
  case "$state" in
    none) echo "create+notify" ;;        # first appearance → open + @mention
    closed) echo "reopen+notify" ;;      # never silent while stale (incl. manual close)
    open)
      # Notify iff at least one TR is newly stale (current \ prior non-empty);
      # otherwise the set is unchanged or shrinking-but-still-stale → silent edit.
      local newly
      newly=$(comm -23 <(printf '%s\n' "$current") <(printf '%s\n' "$prior"))
      if [ -n "$newly" ]; then echo "edit+notify"; else echo "edit"; fi
      ;;
    *) echo "error: unknown issue state '$state'" >&2; return 2 ;;
  esac
}

# Render the issue body (a silent dashboard) from the current report + marker.
render_body() {
  local file="$1" stale="$2"
  local marker_codes
  marker_codes=$(printf '%s' "$stale" | tr '\n' ',' | sed 's/,$//')
  {
    echo "Automated evidence-freshness dashboard for **Recommended** TRs past the"
    echo "90-day backstop. Rewritten every scheduled run."
    echo
    echo "> This is **not** an SDK work item. Escalation stays human (ADR 0013):"
    echo "> re-attest a flagged TR (rerun its Paper Live Smoke, refresh evidence +"
    echo "> \`last_reviewed\`, regenerate docs) and the next run clears it."
    echo
    echo "| TR | last_reviewed | age (days) |"
    echo "|----|---------------|------------|"
    jq -r '.stale[] | "| \(.tr_code) | \(.last_reviewed) | \(.age_days) |"' "$file"
    echo
    echo "_As of $(jq -r '.as_of' "$file") · window $(jq -r '.window_days' "$file") days · $(jq -r '.recommended_count' "$file") Recommended TR(s) examined._"
    echo
    echo "${MARKER_PREFIX} ${marker_codes} ${MARKER_SUFFIX}"
  }
}

# The notifying comment text for a transition into staleness. The maintainer
# handle is validated to a plain `@user` or `@org/team` shape before use — a
# malformed/abusive repo-variable value degrades to no mention rather than
# injecting arbitrary markdown into the issue.
render_notify_comment() {
  local count="$1"
  local who
  who=$(sanitize_handle "${FRESHNESS_MAINTAINER:-}")
  printf '%s evidence freshness needs attention: %s Recommended TR(s) are past the 90-day backstop. See the dashboard above.' "$who" "$count"
}

# Echo the argument only if it is a well-formed GitHub @handle or @org/team;
# otherwise echo nothing. Keeps an untrusted repo variable from injecting content.
sanitize_handle() {
  local who="$1"
  if [[ "$who" =~ ^@[A-Za-z0-9-]+(/[A-Za-z0-9._-]+)?$ ]]; then
    printf '%s' "$who"
  fi
}

# --- gh I/O (live path) ----------------------------------------------------

ensure_label() {
  # Idempotent: creating an existing label is not an error we surface. Without
  # this the inaugural `gh issue create --label` would fail and trip R9.
  gh label create "$LABEL" --color "$LABEL_COLOR" --description "$LABEL_DESC" >/dev/null 2>&1 || true
}

resolve_issue_live() {
  # Print "STATE NUMBER" (STATE lowercased) on line 1, then the issue body on the
  # following lines. The caller captures the whole stream with a command
  # substitution and an explicit status check — the body is emitted on stdout, NOT
  # via a global, because the caller runs this function in a subshell where a
  # global assignment would never reach the parent.
  local json count
  json=$(gh issue list --label "$LABEL" --state all --json number,state,body --limit 1)
  count=$(printf '%s' "$json" | jq 'length')
  if [ "$count" -eq 0 ]; then
    echo "none 0"
    return 0
  fi
  local state number
  state=$(printf '%s' "$json" | jq -r '.[0].state' | tr '[:upper:]' '[:lower:]')
  number=$(printf '%s' "$json" | jq -r '.[0].number')
  echo "$state $number"
  # Remaining lines = the issue body (parse_prior_stale reads its marker).
  printf '%s' "$json" | jq -r '.[0].body'
}

# --- orchestration ---------------------------------------------------------

main() {
  if [ ! -f "$findings" ]; then
    echo "error: findings file not found: $findings" >&2
    exit 2
  fi

  local current stale_count
  current=$(compute_stale_set "$findings")
  stale_count=$(printf '%s' "$current" | grep -c . || true)

  local state number prior
  if $dry_run; then
    state="${FRESHNESS_DRYRUN_ISSUE_STATE:-none}"
    number="${FRESHNESS_DRYRUN_ISSUE_NUMBER:-0}"
    prior=$(parse_prior_stale "${FRESHNESS_DRYRUN_ISSUE_BODY:-}")
  else
    ensure_label
    # Command substitution (not `< <(...)`) so a gh/jq failure inside the resolver
    # surfaces here as a non-zero status, and so the body it prints actually
    # reaches us. Line 1 is "STATE NUMBER"; lines 2+ are the issue body.
    local info
    info=$(resolve_issue_live) || {
      echo "error: could not resolve freshness issue state" >&2
      exit 2
    }
    read -r state number <<<"$(printf '%s\n' "$info" | sed -n '1p')"
    prior=$(parse_prior_stale "$(printf '%s\n' "$info" | tail -n +2)")
  fi

  local action
  action=$(decide_action "$current" "$state" "$prior")

  if $dry_run; then
    echo "ACTION: $action"
    return 0
  fi

  # Single temp file for the rewritten body, cleaned up on every exit path
  # (including a mid-run gh failure) so nothing leaks.
  BODY_FILE=$(mktemp)
  trap 'rm -f "${BODY_FILE:-}"' EXIT

  case "$action" in
    noop) ;;
    create+notify)
      ensure_label
      local num
      render_body "$findings" "$current" >"$BODY_FILE"
      num=$(gh issue create --label "$LABEL" --title "$TITLE" --body-file "$BODY_FILE" \
        | grep -oE '[0-9]+$' | tail -n1)
      gh issue comment "$num" --body "$(render_notify_comment "$stale_count")"
      ;;
    edit|edit+notify)
      render_body "$findings" "$current" >"$BODY_FILE"
      gh issue edit "$number" --body-file "$BODY_FILE"
      # Explicit conditional (not `[ ] && cmd`): a false test there would make the
      # silent-edit branch — the steady-state outcome — return 1 and spuriously
      # trip the workflow's R9 failure path on a perfectly healthy run.
      if [ "$action" = "edit+notify" ]; then
        gh issue comment "$number" --body "$(render_notify_comment "$stale_count")"
      fi
      ;;
    reopen+notify)
      render_body "$findings" "$current" >"$BODY_FILE"
      gh issue reopen "$number"
      gh issue edit "$number" --body-file "$BODY_FILE"
      gh issue comment "$number" --body "$(render_notify_comment "$stale_count")"
      ;;
    close)
      gh issue comment "$number" --body "All clear: every Recommended TR is within the 90-day backstop. Closing the freshness dashboard."
      gh issue close "$number"
      ;;
    *)
      echo "error: unhandled action '$action'" >&2
      exit 2
      ;;
  esac
}

# Run the orchestrator only when executed directly; sourcing (the unit tests)
# exposes the pure helpers without triggering any I/O.
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  main "$@"
fi
