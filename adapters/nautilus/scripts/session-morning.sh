#!/usr/bin/env bash
# session-morning.sh — U4: the 08:45–09:15 KST chain as ONE pre-staged entry point.
#
# WHY THIS EXISTS. Head v34 builds its opening range only from live bars observed between
# 09:00 and 09:15 KST — there is no backfill, so a mount that starts at 09:16 takes zero
# trades. The chain that has to finish first is six long environment-laden commands spread
# across two documents. Assembling those under a 30-minute clock is how 2026-07-29 was
# lost. Everything below is resolved AHEAD of the clock: absolute paths, real binaries, the
# symbol list read from the checkpoint, and a pace check that stands the run down early
# rather than handing over a universe that arrives too late to use.
#
# THREE CLOCKS, distinct on purpose:
#   09:05  the ingest must be DONE                (LS_SM_INGEST_BY)
#   09:10  the resolved universe must be IN HAND  (LS_SM_UNIVERSE_BY)
#   09:15  the opening range opens                — what the other two exist to protect
#
# TWO CALLERS, and until --catch-up they were indistinguishable. The ATTENDED caller is racing
# those clocks and a universe that lands late is worthless, so killing a doomed ingest is right.
# The CATCH-UP caller has already conceded the mount — it runs on a weekend or after the close
# purely to advance the calendar and the catalog. For it the pace kill is the failure: it leaves
# the partial watermark distribution the catch-up exists to prevent. `--catch-up` disables the
# step [7] kill and ONLY that; the step [8] universe refusal stays, because standing down before
# resolving a universe is the SUCCESS path on a catch-up rather than a concession.
#
# WHAT THIS SCRIPT WILL NEVER DO. It never runs `--mount`, `--dispatch`, or `--genesis`,
# and it never authors the attended Unknown override's `operator`, `reason`, or `citation`
# fields. Those are operator-only, nonce-gated, and TTY-gated by design. This script stops
# at a GO/NO-GO report and hands the operator a checklist.
#
# Usage:
#   ./session-morning.sh --dry-run              # print the resolved sequence, zero traffic
#   ./session-morning.sh --self-test            # exercise the pace check, zero traffic
#   ./session-morning.sh                        # run it (attended: racing the 09:15 clock)
#   ./session-morning.sh --catch-up             # run it with the mount conceded (see below)
#   ./session-morning.sh --stop-before-activate # stop after the diff for a manual review
#
# Env (all optional — every default is resolved below and printed by --dry-run):
#   LS_SM_SESSION_DATE  session to ingest, the PREVIOUS session      (default 2026-07-29)
#   LS_SM_MOUNT_DATE    session to resolve a universe FOR, today     (default 2026-07-30)
#   LS_SM_INGEST_BY     ingest-completion target HH:MM local         (default 09:05)
#   LS_SM_UNIVERSE_BY   universe-in-hand target HH:MM local          (default 09:10)
#   LS_SM_NOW           override "now" as HH:MM — pace testing ONLY  (default: real clock)
#   LS_SM_POLL_SECS     ingest progress poll interval, 1..30 seconds (default 30)
#   LS_SM_OPERATOR      operator id written into the calendar approval (default sunkeunchoi)
#   LS_SM_LOOKBACK      ingest coverage floor YYYYMMDD               (default 20260518)
#   LS_SM_ALLOW_STALE_BINARIES  0|1 — proceed on DELIBERATELY PINNED binaries (default 0).
#                       Allowed on a real run and announced in the transcript. Bypasses the
#                       preflight mtime axis ONLY; a missing registered guard is never bypassable.
#
# Exit codes (the contract — never read success from log text):
#   0   GO      — universe resolved (or a valid flat-open refusal), report delivered
#   1   NO-GO   — a step failed in a way the runbook anticipates; state reported
#   40  STAND-DOWN — not on pace; abandoned deliberately, before the universe step
#   41  CATCH-UP COMPLETE — --catch-up only. The calendar and the catalog advanced IN FULL and
#       the universe step was refused BY DESIGN. A success, and NOT a stand-down.
#   64  misconfiguration, OR a required binary that is absent, STALE by mtime, of UNEVALUABLE
#       freshness, or missing a registered guard literal — refused before issuing any traffic.
#       An existence check is not a freshness check: a binary older than the sources it was
#       built from runs code nobody is reading, and on 2026-08-04 that reported `ok` twelve times.
#
# WHY 41 AND NOT 0 OR 40. Exit 0 is load-bearing: it asserts "a resolved universe is in hand",
# which is what a wrapper acts on, and a catch-up resolves none — returning 0 would make the one
# thing a caller is allowed to read into a lie. Exit 40 means "ran out of clock, work abandoned
# mid-flight, retry"; a completed catch-up means "everything it set out to do is done, nothing
# left to retry". Collapsing those two would force a caller to tell them apart by reading log
# text, which this contract exists to forbid. 41 sits in the same decade as 40, so the coarse
# rule `rc != 0 => no universe in hand` still holds, while `rc == 41` stays separately readable.
set -uo pipefail

# ---------------------------------------------------------------- paths (all absolute)
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
R="$(cd -- "$script_dir/../../.." && pwd)"          # the ONE repo-root variable
NAUT="$R/adapters/nautilus"
BIN="$NAUT/target/debug"
STATE="$NAUT/state"
DATA_HOME="$R/data/turn4-fresh"
CATALOG="$DATA_HOME/catalog"
CKPT="$CATALOG/ingest-checkpoint.json"
SNAPSHOT="$STATE/krx.calendar.json"
LANE_ENV="$R/.env.domestic"
ENV_CALENDAR="$R/.env.calendar"
UNIVERSE_METADATA="$NAUT/lab/config/universe-metadata-20260723.json"
WITNESS_LOG="$script_dir/krx-witness-watch.log"

session_date="${LS_SM_SESSION_DATE:-2026-07-29}"      # ingest THIS session
mount_date="${LS_SM_MOUNT_DATE:-2026-07-30}"          # resolve a universe FOR this one
session_compact="${session_date//-/}"
mount_compact="${mount_date//-/}"
ingest_by="${LS_SM_INGEST_BY:-09:05}"
universe_by="${LS_SM_UNIVERSE_BY:-09:10}"
lookback="${LS_SM_LOOKBACK:-20260518}"
operator="${LS_SM_OPERATOR:-sunkeunchoi}"
OUT_UNIVERSE="$DATA_HOME/mount-universe-$mount_compact.json"
APPROVAL="$STATE/refresh-$(date +%Y%m%d).approval.json"   # keyed on RUN date, not through-date
INPUTS="$STATE/refresh-$(date +%Y%m%d).calendar-inputs.json"
# FETCH_CKPT is defined AFTER the window derivation below — its name is keyed on the
# full derived window, because calendar-fetch-inputs refuses to resume a checkpoint
# whose window differs from the run's (CheckpointMismatch). See the derivation comment.
CANDIDATE="$SNAPSHOT.candidate"
CANDIDATE_DIFF="$SNAPSHOT.candidate.diff.json"
ARCHIVE="$SNAPSHOT.archive-$(date +%Y%m%d)"

dry_run=0; self_test=0; stop_before_activate=0; catch_up=0
for a in "$@"; do case "$a" in
  --dry-run) dry_run=1 ;;
  --self-test) self_test=1 ;;
  --stop-before-activate) stop_before_activate=1 ;;
  --catch-up) catch_up=1 ;;
  *) echo "error: unknown argument '$a'" >&2; exit 64 ;;
esac; done

# Poll interval for the step [7] progress loop. Unlike LS_SM_NOW this is NOT refused on a real
# run, and it does not need to be: it is a LATENCY knob, not an input to any decision. Every
# argument pace_verdict receives — advanced, total, elapsed, now, deadline — is derived from the
# real clock and the real checkpoint, so a shorter interval changes only how soon the same
# verdict is noticed, never which verdict it is. The 1..30 bound is what keeps that true at both
# ends: it cannot be set low enough to hammer the checkpoint into permanent mid-write reads, nor
# high enough to stall a stand-down past its own deadline. A malformed value is refused loudly
# rather than silently falling back, so a typo cannot quietly restore the 30s default.
poll_secs="${LS_SM_POLL_SECS:-30}"
if ! [[ "$poll_secs" =~ ^[0-9]+$ ]] || (( poll_secs < 1 || poll_secs > 30 )); then
  echo "error: LS_SM_POLL_SECS must be an integer in 1..30 (got '$poll_secs')." >&2
  exit 64
fi

# The operator's escape from the preflight mtime axis, and from THAT AXIS ONLY.
#
# Modelled on LS_SM_POLL_SECS above, not on LS_SM_NOW: it is NOT refused on a real run, and it
# must not be. A deliberately pinned binary is a legitimate operator state — queue item
# `session-morning-20260730` records a run whose binaries were pinned at 5f38144 with an explicit
# DO NOT REBUILD — and a test-only seam would leave that operator no route but to edit this script
# under the 09:05 clock. The chain runs at 08:45, and any `git` operation that touches a source
# file trips the mtime axis whether or not content changed, so the escape has to be reachable
# where the cost actually lands.
#
# It covers the mtime axis alone. The content axis asserts that a KNOWN GUARD is present in the
# artifact, and a binary pinned on purpose is still pinned to code containing its registered
# guard, so nothing legitimate needs that escape. Binding both axes to one switch would let the
# noisy axis train the operator into disabling the quiet one — and `touch target/debug/*`, the
# cheapest response to a false-stale, is exactly what produces the mtime inversion only the
# content axis can see.
#
# A malformed value is refused rather than defaulted, in BOTH directions: silently falling back
# to 0 strands an operator who believes they bypassed the check, and silently falling back to 1
# would disable the check for a typo.
allow_stale_bins="${LS_SM_ALLOW_STALE_BINARIES:-0}"
if [[ "$allow_stale_bins" != "0" && "$allow_stale_bins" != "1" ]]; then
  echo "error: LS_SM_ALLOW_STALE_BINARIES must be 0 or 1 (got '$allow_stale_bins')." >&2
  exit 64
fi

say()  { printf '%s  %s\n' "$(date '+%H:%M:%S')" "$*"; }
step() { printf '\n=== %s ===\n' "$*"; }
die()  { printf '\nNO-GO: %s\n' "$*" >&2; exit 1; }

# Daily symbols whose watermark has reached $session_compact, read from the checkpoint.
# Echoes -1 when the checkpoint cannot be read (mid-write, absent, corrupt) so a caller can
# tell "unknown" from "zero" — a silent 0 would read as a stalled ingest.
#
# Two things this deliberately gets right, both of which the Rust half already gets right:
#   * `>=`, not `==`. On a proven-Closed range ls-ingest SkipAdvances watermarks to the last
#     closed session, so a Monday run writes Sunday's date. An equality test then counts zero
#     advanced and reports a partial ingest on a complete catalog. YYYYMMDD strings sort
#     chronologically, so a lexicographic `>=` is the whole fix.
#   * `|1-DAY`, not `1-DAY`. Checkpoint keys are `{instrument}|{bar_type}`; anchoring on the
#     separator is what `Checkpoint::watermarks_for` does, and its doc comment explicitly
#     warns that the bare suffix is documentation rather than truth.
count_advanced() {
  python3 -c "
import json,sys
try:
    w=json.load(open(sys.argv[1]))['watermarks']
except Exception:
    print(-1); sys.exit()
print(sum(1 for k,v in w.items() if k.endswith('|1-DAY') and v >= sys.argv[2]))" \
    "$CKPT" "$session_compact" 2>/dev/null || echo -1
}

# A killed ls-ingest leaves $CATALOG/.ls-ingest.lock behind — the lock is RAII-removed on
# drop, and a stale one deliberately blocks every later run until an operator clears it. Any
# path that kills the ingest and then tells the operator a re-run is safe must clear it, or
# that promise is false. turn4-ingest.sh already does this before each attempt.
clear_ingest_lock() { rm -f "$CATALOG/.ls-ingest.lock"; }

# ------------------------------------------------------------------- the pace evaluator
# Pure arithmetic, no I/O, so --self-test can drive it with a simulated clock. Projects
# ingest completion from OBSERVED throughput and compares it to the ingest deadline.
# Echoes "<verdict> <projected_finish_HH:MM> <minutes_remaining_to_deadline> <eta_minutes>".
pace_verdict() {  # $1=advanced $2=total $3=elapsed_s $4=now_epoch $5=deadline_epoch
  python3 -c "
import sys,datetime
adv,tot,el,now,dl = int(sys.argv[1]),int(sys.argv[2]),float(sys.argv[3]),int(sys.argv[4]),int(sys.argv[5])
remain_min = (dl-now)/60.0
if adv >= tot:
    print('GO', datetime.datetime.fromtimestamp(now).strftime('%H:%M'), round(remain_min,1), 0.0); raise SystemExit
if adv <= 0 or el <= 0:
    # No throughput observed yet. Only a verdict once the deadline itself has passed —
    # standing down on a slow first minute would be its own failure mode.
    print('LATE' if now >= dl else 'WAIT',
          datetime.datetime.fromtimestamp(dl).strftime('%H:%M'), round(remain_min,1), -1.0); raise SystemExit
rate = adv/el                      # symbols per second, observed
eta_s = (tot-adv)/rate             # seconds of work left at that rate
finish = now + eta_s
print('GO' if finish <= dl else 'LATE',
      datetime.datetime.fromtimestamp(finish).strftime('%H:%M'),
      round(remain_min,1), round(eta_s/60.0,1))" "$1" "$2" "$3" "$4" "$5"
}

# Local HH:MM -> epoch seconds for TODAY. LS_SM_NOW overrides "now" for pace testing only;
# it never reaches a network call or an artifact write.
hhmm_epoch() { python3 -c "
import datetime,sys
h,m=sys.argv[1].split(':')
print(int(datetime.datetime.now().replace(hour=int(h),minute=int(m),second=0,microsecond=0).timestamp()))" "$1"; }
now_epoch() { if [[ -n "${LS_SM_NOW:-}" ]]; then hhmm_epoch "$LS_SM_NOW"; else date +%s; fi; }

# ------------------------------------------------------------------------- --self-test
if (( self_test )); then
  step "self-test: pace check (no network, no artifacts)"
  ing_dl="$(hhmm_epoch "$ingest_by")"
  printf '%-58s -> %s\n' "on pace: 60/75 done, 8 min elapsed, now 08:58" \
    "$(pace_verdict 60 75 480 "$(hhmm_epoch 08:58)" "$ing_dl")"
  printf '%-58s -> %s\n' "AE3 late: 20/75 done, 10 min elapsed, now 08:58" \
    "$(pace_verdict 20 75 600 "$(hhmm_epoch 08:58)" "$ing_dl")"
  printf '%-58s -> %s\n' "AE3 late: 40/75 done, 15 min elapsed, now 09:02" \
    "$(pace_verdict 40 75 900 "$(hhmm_epoch 09:02)" "$ing_dl")"
  printf '%-58s -> %s\n' "complete: 75/75" \
    "$(pace_verdict 75 75 900 "$(hhmm_epoch 09:02)" "$ing_dl")"
  printf '%-58s -> %s\n' "no throughput yet, before deadline" \
    "$(pace_verdict 0 75 60 "$(hhmm_epoch 08:56)" "$ing_dl")"
  printf '%-58s -> %s\n' "no throughput, deadline already passed" \
    "$(pace_verdict 0 75 600 "$(hhmm_epoch 09:07)" "$ing_dl")"
  echo
  echo "LATE at or before the deadline is the AE3 stand-down trigger; WAIT defers a verdict"
  echo "until throughput exists so a slow first poll cannot stand the run down on its own."
  exit 0
fi

# ------------------------------------------------------------------- the freshness axes
# A binary older than the sources it was built from runs code nobody is reading. On 2026-08-04
# the tree was clean at 92ba1ed while $BIN/calendar-refresh was built 19 minutes BEFORE
# src/bin/calendar-refresh.rs and src/calendar_refresh/candidate.rs — and the preflight reported
# `ok` for all twelve paths. Git operations touch sources after a build, so a clean tree is no
# evidence that the binaries match it. Had that run continued it would have executed a
# calendar-refresh predating PR #258's forward-horizon guard, whose entire purpose is to make a
# refusal OBSERVABLE — so the missing refusal line would have read as a clean pass.
#
# TWO AXES, because neither subsumes the other. mtime is the general freshness signal; the content
# literal proves a specific behavior is IN the artifact, which mtime cannot. The literal is the
# deciding axis in exactly one reachable state — mtime INVERSION, where a binary is newer than
# every source yet built from older code. That state is produced by a build racing a `git pull`, a
# build made in another worktree or branch, or the cheapest operator response to a false-stale
# (`touch target/debug/*`). The last of those is why the content axis sits outside the override's
# reach. See docs/solutions/workflow-issues/first-run-of-a-new-guard-prove-the-binary-then-\
# discharge-its-residual.md, which prescribes the pairing and names this residual.

# Freshness inputs for ONE binary, read from the dep-info file cargo writes beside every artifact
# ($BIN/<name>.d). Echoes "<binary_mtime> <newest_source_mtime> <vanished_source_count>", the
# first two epoch seconds — or "-1 -1 -1" when the answer is UNKNOWN (no dep-info file, an
# unreadable one, or not one usable source path inside it). The -1 sentinel mirrors
# count_advanced's, for the same reason: this script runs under `set -uo pipefail` with NO `-e`,
# so a failed scan neither aborts nor refuses on its own, and an empty result that reads as
# "fresh" would be the very false green this axis exists to close.
#
# WHY CARGO'S DEP-INFO RATHER THAN A HAND-LISTED SOURCE SCAN. Cargo's per-binary set is the only
# one that gets all three of these right at once, and it cannot drift as the dependency graph does:
#   * PER-BINARY, so rebuilding one stale binary clears its own refusal. A single newest-source
#     value shared across all seven is unrecoverable — cargo relinks only DIRTY targets, so the
#     rebuild freshens the touched binary and leaves the other six behind the new shared value,
#     with cargo declining to rebuild them. The live tree shows the spread this cannot tolerate:
#     the five nautilus-ls binaries and the two lab binaries sit an hour apart, all correct.
#   * It reaches what a src/ scan cannot. crates/ls-core/build.rs embeds the repo-root metadata/
#     tree at compile time, so a metadata/constraints/*.yaml edit changes every binary's behavior
#     while moving no file under any src/ directory. calendar-refresh.d lists those paths twelve
#     times; no src/ scan would ever reach them.
#
# WHAT DEP-INFO OMITS, AND WHY THE MANIFESTS ARE ADDED BY HAND. Cargo records SOURCE FILES only:
# `calendar-refresh.d` contains zero `Cargo.toml`, zero `Cargo.lock`, and zero toolchain entries
# (verified over all 135 of its paths). So a MANIFEST-ONLY change — a dependency bump, a feature
# flip, `cargo update` rewriting the lockfile, a `rust-toolchain.toml` edit — makes cargo consider
# every binary dirty while every path it recorded stays older than the artifact. Dep-info alone
# therefore reports `ok` for seven binaries built from superseded dependency code, and the content
# axis cannot help: a manifest change removes no registered literal. That is a false green of
# exactly the class this preflight exists to close, so the manifests are folded in explicitly.
# They are a genuinely closed set — one per workspace member plus the two lockfiles and the pinned
# toolchain — not a source-tree scan, so KTD8's objection to a hand-listed scan does not apply: a
# new workspace member is a repo-structure change, not a routine edit, and a manifest that goes
# missing counts toward `vanished` so a typo here fails CLOSED rather than silently covering less.
BIN_EXTRA_FRESHNESS_INPUTS=(
  "$R/Cargo.toml" "$R/Cargo.lock"
  "$NAUT/Cargo.toml" "$NAUT/Cargo.lock" "$NAUT/rust-toolchain.toml"
  "$NAUT/lab/Cargo.toml" "$NAUT/nautilus-ls-calendar/Cargo.toml"
)
#   * It does not OVER-report. A hand list broad enough to be safe would include
#     adapters/nautilus/lab/src, which calendar-refresh.d references ZERO times — every lab edit
#     would then mark the calendar binaries stale inside the 09:05 deadline, for a dependency
#     that does not exist.
#
# This is NOT delegating the verdict to cargo. Cargo has no check-only mode, so delegating would
# mean auto-remediation instead of refusal — contradicting both the exit-64 contract and the
# DO NOT REBUILD precedent above — and an unbounded one at that: measured 0.4s steady-state, but
# 41s for the first `--bins` after `cargo test`, and minutes when a root crate relinks two ~260 MB
# binaries. Reading metadata cargo has ALREADY persisted, and refusing on it, is neither a rebuild
# nor a handoff of the decision.
#
# The honest limit: a source file added since the last build is absent from the .d, so dep-info is
# authoritative about what the binary WAS built from — which is exactly the question being asked.
#
# NANOSECOND comparison, not whole seconds. `int(st_mtime)` truncates both sides, so a source
# written up to 0.99s AFTER the binary inside the same integer second reads as fresh — the
# sub-second form of "a build racing a git pull", and one the content axis cannot cover for the five
# unregistered binaries. `st_mtime_ns` is an int, so bash's 64-bit `(( ))` still handles it (and
# will until the year 2262); the -1 sentinel is unaffected.
dep_freshness() { # $1 = absolute path to the binary; $2.. = extra inputs cargo's dep-info omits
  python3 -c '
import os, re, sys
binary = sys.argv[1]
extra = sys.argv[2:]
try:
    binary_mtime = os.stat(binary).st_mtime_ns
except OSError:
    print("-1 -1 -1"); raise SystemExit
try:
    with open(binary + ".d") as handle:
        rules = handle.read()
except OSError:
    print("-1 -1 -1"); raise SystemExit
newest, seen, vanished = -1, 0, 0
def consider(path):
    global newest, seen, vanished
    try:
        mtime = os.stat(path).st_mtime_ns
    except OSError:
        # A recorded input that is gone. Cargo itself calls such a target dirty, so the binary is
        # stale even when every surviving input is older than it.
        vanished += 1
        return
    seen += 1
    newest = max(newest, mtime)
# A .d file is a make rule: "<target>: <src> <src> ...". Cargo may also emit bare "<src>:" lines;
# every .d in this tree is a single rule line, so that branch is defensive. Everything after the
# first colon on a line is a source list -- space-separated, with make backslash escaping, and
# every path ABSOLUTE. So this is a split, not a dependency-graph walk.
for line in rules.splitlines():
    _, sep, sources = line.partition(":")
    if not sep:
        continue
    for path in re.split(r"(?<!\\)\s+", sources.strip()):
        if path:
            consider(path.replace("\\ ", " "))
# The manifests and lockfiles cargo records nowhere. A missing one counts as vanished, so this
# list fails CLOSED: a typo or a moved manifest refuses rather than quietly covering less.
for path in extra:
    consider(path)
if not seen:
    print("-1 -1 -1"); raise SystemExit
print(binary_mtime, newest, vanished)' "$1" "${@:2}" 2>/dev/null || echo "-1 -1 -1"
}

# THE PROBE-LITERAL REGISTRY — the content axis's whole input, and deliberately SPARSE (a binary
# with no entry passes this axis). One entry per line, "<binary-name>|<literal>|<provenance>",
# where the provenance records the PR that introduced the literal so a future reader can tell what
# the assertion is protecting rather than guessing.
#
# FORMAT CONSTRAINT: `|` is the field separator, so a literal containing `|` is silently TRUNCATED
# by probe_literal_for below and by the test harness's own parse — both would agree on the wrong
# value, so no test could catch it. `make script-check` asserts the field count for this reason;
# pick a literal without a pipe, or change the separator here and in registry_entries together.
#
# Choose a literal for UNIQUENESS, and test PRESENCE rather than a count. `grep -c forward_horizon`
# returns 3 today only because the compiler did not merge three verdict literals sharing that
# prefix — an unrelated edit could collapse it to 1 and turn a healthy binary red. Presence is
# immune to merge behavior.
#
# COST, measured with the grep a non-interactive script actually resolves (`/usr/bin/grep`, BSD):
# 0.05s for the 4.8 MB calendar-refresh, ~1.8s for a HIT on the 251 MB ls-ingest (grep stops at
# the match), ~4.1s for a full scan when a literal is ABSENT — a full scan is the miss case, and
# it is the miss case that refuses. (An earlier note here read 0.18s; that was measured against an
# interactive shell whose `grep` is aliased to `ugrep`, which the script never uses.) Today's
# registry costs ~1.9s on the healthy path; registering the two remaining large binaries would add
# ~4s-per-large-miss each, so size any growth against that figure rather than the old one.
#
# The registry grows when a GUARD SHIPS, not on a schedule. Registering an arbitrary literal for a
# binary with no recent load-bearing change asserts nothing. The "register the other six" survey
# (queue item, closed 2026-08-12) resolved to ONE registration: ls-ingest carries PR #101's
# append-overlap refusal, the write-side guard for the very ingest this preflight fronts, and a
# stale binary lacking it silently corrupts the catalog rather than failing visibly. The other
# five stay unregistered on judgment, not neglect: calendar-fetch-inputs' load-bearing contract is
# its ARGV, covered by the harness's replay against the real compiled binary (a stronger assertion
# than any literal); calendar-activate, calendar-status, lab-research, and lab-mount-universe have
# shipped no refusal literal whose silent absence is dangerous on this path — when one does,
# register it then. `make script-check` fails on BOTH ends of each entry:
# when a registered literal no longer occurs in the repo's Rust sources (R10 — a reword), and when
# it is absent from the real compiled `target/debug` artifact it is registered for (R11 — a stale
# or inverted build). It is a `make gate-run` step running right after adapter-check builds those
# artifacts, so either is normally PREEMPTED at the commit gate. A gate-less commit can still land
# a reword, so the refusal message below names the entry — the operator can tell a reworded source
# from a stale binary in one line.
BIN_PROBE_LITERALS=(
  "calendar-refresh|REFUSED (asked for|PR #258 forward-horizon guard — the refusal line whose ABSENCE reads as a clean pass"
  "ls-ingest|APPEND REFUSED (overlap)|PR #101 append-overlap refusal — the write-side guard whose ABSENCE lets a stale binary corrupt the catalog silently"
)

# The literal registered for a binary, or empty when it is unregistered.
probe_literal_for() { # $1 = binary basename
  local entry rest
  for entry in ${BIN_PROBE_LITERALS[@]+"${BIN_PROBE_LITERALS[@]}"}; do
    [[ "${entry%%|*}" == "$1" ]] || continue
    rest="${entry#*|}"; printf '%s' "${rest%%|*}"; return
  done
}

# Applies the axes to ONE required binary. Sets $bin_verdict to the word the transcript prints and
# appends the binary's name to the matching per-cause list, which the refusal blocks below read.
# At most ONE cause is recorded per binary, and the order is deliberate: a stale binary reports
# staleness rather than a confusing downstream failure.
check_binary() { # $1 = absolute path to a required binary
  local f="$1" name bin_mtime src_mtime vanished literal pinned=0
  name="${f##*/}"
  if [[ ! -x "$f" ]]; then
    missing=$((missing + 1)); missing_bins+=("$name"); bin_verdict="MISS"; return
  fi
  bin_verdict="ok"
  read -r bin_mtime src_mtime vanished \
    <<<"$(dep_freshness "$f" ${BIN_EXTRA_FRESHNESS_INPUTS[@]+"${BIN_EXTRA_FRESHNESS_INPUTS[@]}"})"
  # UNEVALUABLE is NOT bypassable by the override, and the distinction is the point. "Stale" is a
  # KNOWN state an operator can pin on purpose — that is the whole justification for the escape.
  # "Unevaluable" is this preflight not knowing WHAT it is about to run, and no operator assertion
  # covers that, so the override's reach stops at the stale-by-mtime verdict alone.
  if (( src_mtime < 0 )); then
    unprovable_bins+=("$name"); bin_verdict="NODEP"; return
  fi
  if (( bin_mtime < src_mtime || vanished > 0 )); then
    if (( ! allow_stale_bins )); then
      stale_bins+=("$name"); bin_verdict="STALE"; return
    fi
    # Stale, and taken as deliberately pinned. Reported PER BINARY rather than only through the
    # banner above, so the transcript names exactly which artifacts the operator vouched for —
    # and execution continues to the content axis, which the override never reaches.
    bin_verdict="PIN"; pinned=1
  fi
  # THE CONTENT AXIS, reached only once the mtime axis has PASSED or been overridden. Ordering it
  # last is deliberate: a stale binary must report staleness rather than a literal failure that is
  # merely a downstream symptom of it.
  literal="$(probe_literal_for "$name")"
  if [[ -n "$literal" ]] && ! grep -qaF -- "$literal" "$f"; then
    # Carry the PIN fact into the entry rather than discarding it. A pinned binary that ALSO lacks
    # its guard is not the ambiguous rebuild-or-reword case: the mtime axis already established
    # that it predates its inputs, so the refusal below can name the answer instead of handing the
    # operator an open question at 08:45.
    noliteral_bins+=("$name|$literal|$pinned"); bin_verdict="NOSIG"
  fi
}

# The exact rebuild command, and the workspace it has to run from. adapters/nautilus is a
# STANDALONE workspace, so a bare `cargo build` at the repo root resolves against the OTHER one
# and cannot produce these binaries at all; and --workspace is required even inside it, because
# lab-research and lab-mount-universe live in the `lab` member rather than the default-run
# package (`cargo build --bin lab-research` alone fails with "no bin target ... in default-run
# packages"). One form that works for all seven beats a per-binary package mapping that can drift.
#
# ONE command covering every named binary, not one per binary. The cross-workspace case this design
# targets — a `crates/ls-core` or `metadata/` edit — implicates all seven at once, because all seven
# dep-info sets share those paths. Printing seven sequential `cargo build` invocations there would
# hand the operator seven separate relinks under the 09:05 clock when a single invocation rebuilds
# them together and shares the compile.
rebuild_hint() { # $1.. = binary names
  local b targets=""
  for b in "$@"; do targets="$targets --bin $b"; done
  echo "       rebuild, from the adapters/nautilus workspace:" >&2
  echo "         (cd $NAUT && cargo build --workspace$targets)" >&2
}

# ------------------------------------------------------------------------ preflight
step "preflight"
if [[ "${LS_TRADING_ENV:-}" != "paper" ]]; then
  echo "error: LS_TRADING_ENV must be exactly 'paper' (got '${LS_TRADING_ENV:-<unset>}')." >&2
  echo "       This chain is a paper rehearsal; it refuses to resolve against a live lane." >&2
  exit 64
fi
# LS_SM_NOW is a test seam. Refusing it on a real run keeps the line-112 contract true by
# construction rather than by discipline — a stale export from an earlier pace test is
# exactly the way a seam like this reaches production.
if [[ -n "${LS_SM_NOW:-}" ]] && (( ! dry_run && ! self_test )); then
  echo "error: LS_SM_NOW is a pace-testing seam and must not be set on a real run." >&2
  echo "       Unset it, or pass --dry-run / --self-test." >&2
  exit 64
fi
# TWO CLASSES, discriminated by LOCATION rather than by a second hand-maintained list.
# Seven of these twelve paths are compiled artifacts under $BIN and five are state or config
# the chain reads; only the former can be STALE, so only the former carry the freshness axes
# below. Classifying on the `$BIN/` prefix is what keeps the two sets from drifting: the next
# binary added joins this one loop and picks up the freshness axes for free, whereas a separate
# binary list would let it join the existence check and silently skip freshness, with no signal.
# The binary class also tests -x rather than -e — an unexecutable artifact is as unusable as an
# absent one, and turn4-ingest.sh already refuses on -x for exactly this reason.
if (( allow_stale_bins )); then
  say "LS_SM_ALLOW_STALE_BINARIES=1 — the preflight mtime axis is BYPASSED for this run."
  say "  ALL SEVEN required binaries are taken as deliberately pinned: the switch has no"
  say "  per-binary form, so pinning one artifact waives the mtime evidence for the other six too."
  # State the residual coverage HONESTLY rather than reassuringly. "The content axis still applies"
  # is true but nearly vacuous while the registry covers a minority of the seven — the unregistered
  # binaries have NO freshness evidence at all on an overridden run, and an operator deciding
  # whether that is acceptable needs the count, not the principle.
  say "  Residual coverage this run: the content axis, which is NOT bypassable — but it is"
  say "  registered for ${#BIN_PROBE_LITERALS[@]} of 7 binaries, so the rest carry no freshness evidence at all."
fi
missing=0
missing_bins=(); stale_bins=(); unprovable_bins=(); noliteral_bins=()
for f in "$BIN/calendar-fetch-inputs" "$BIN/calendar-refresh" "$BIN/calendar-activate" \
         "$BIN/calendar-status" "$BIN/ls-ingest" "$BIN/lab-research" "$BIN/lab-mount-universe" \
         "$SNAPSHOT" "$LANE_ENV" "$ENV_CALENDAR" "$UNIVERSE_METADATA" "$CKPT"; do
  if [[ "$f" == "$BIN/"* ]]; then
    check_binary "$f"                       # sets $bin_verdict, appends to the per-cause lists
    say "$(printf '%-4s' "$bin_verdict") $f"
  else
    if [[ -e "$f" ]]; then say "ok   $f"; else say "MISS $f"; missing=$((missing+1)); fi
  fi
done

# THE FOUR REFUSAL CAUSES, each carrying its OWN remedy. Collapsing them into one
# "rebuild and re-run" line would repeat the misattribution recorded in
# docs/solutions/workflow-issues/shell-script-live-path-needs-stubbed-binary-tests.md: a handler
# must discriminate among all the ways it can fire, not assert the one cause its author had in
# mind. Every arm is reached before any gateway traffic, and every arm exits 64 by hand — `die` is
# hardwired to exit 1, so it cannot carry this verdict, and 64 is already this script's
# preflight-refusal code (see the exit-code contract in the header).
refused=0
if (( missing )); then
  echo "error: $missing required path(s) missing" >&2
  if (( ${#missing_bins[@]} )); then
    echo "       ABSENT BINARY — the chain cannot run what is not there (or is not executable)." >&2
    rebuild_hint "${missing_bins[@]}"
  fi
  refused=1
fi
if (( ${#unprovable_bins[@]} )); then
  echo "error: freshness UNEVALUABLE for ${#unprovable_bins[@]} required binary(ies):" \
       "${unprovable_bins[*]}" >&2
  echo "       Each binary's source set is read from the dep-info file cargo writes beside it" >&2
  echo "       (\$BIN/<name>.d). A missing, unreadable, or source-less one means this preflight" >&2
  echo "       cannot tell a current binary from a stale one — and an unevaluable check must" >&2
  echo "       never fall through to pass. Rebuilding regenerates the dep-info file." >&2
  rebuild_hint "${unprovable_bins[@]}"
  refused=1
fi
if (( ${#stale_bins[@]} )); then
  echo "error: ${#stale_bins[@]} required binary(ies) are STALE: ${stale_bins[*]}" >&2
  echo "       Older than a source cargo recorded building them from, or built from a source" >&2
  echo "       that no longer exists. A clean tree is NOT evidence — git operations touch" >&2
  echo "       sources after a build, which is exactly how 2026-08-04 reported ok twelve times." >&2
  rebuild_hint "${stale_bins[@]}"
  echo "       Only the named binaries are implicated: each is compared against its OWN" >&2
  echo "       dep-info set, so rebuilding these clears the refusal and leaves the rest alone." >&2
  echo "       Deliberately pinned binaries: LS_SM_ALLOW_STALE_BINARIES=1 bypasses THIS axis" >&2
  echo "       only — never an absent binary, never a missing registered guard." >&2
  refused=1
fi
if (( ${#noliteral_bins[@]} )); then
  echo "error: ${#noliteral_bins[@]} required binary(ies) are missing their REGISTERED GUARD literal:" >&2
  noliteral_names=(); any_unpinned=0
  for e in "${noliteral_bins[@]}"; do
    nl_name="${e%%|*}"; nl_rest="${e#*|}"
    nl_literal="${nl_rest%|*}"; nl_pinned="${nl_rest##*|}"
    noliteral_names+=("$nl_name")
    if (( nl_pinned )); then
      # The mtime axis already proved this one predates its inputs, so do NOT present the
      # rebuild-or-reword ambiguity below as an open question for it — the answer is known.
      echo "         BIN_PROBE_LITERALS entry '$nl_name' expects: $nl_literal" >&2
      echo "           ^ this binary is ALSO stale by mtime and was let through only by" >&2
      echo "             LS_SM_ALLOW_STALE_BINARIES. The cause is not ambiguous: REBUILD it." >&2
    else
      echo "         BIN_PROBE_LITERALS entry '$nl_name' expects: $nl_literal" >&2
      any_unpinned=1
    fi
  done
  if (( any_unpinned )); then
    echo "       TWO causes, needing OPPOSITE fixes — tell them apart before acting:" >&2
    echo "         * the binary predates the guard, or was built from another tree. The mtime axis" >&2
    echo "           cannot see that: an INVERTED binary is newer than every source yet built from" >&2
    echo "           older code, which is what a build racing a git pull, a build in another" >&2
    echo "           worktree, or \`touch target/debug/*\` produces. Rebuild." >&2
    echo "         * the SOURCE was reworded, so the registry entry above is now stale. Left alone" >&2
    echo "           that is a permanent exit 64 on every morning until the entry is updated." >&2
    echo "       \`make script-check\` decides which: it asserts that every registered literal still" >&2
    echo "       occurs in the repo's Rust sources. A FAILURE there means fix the registry entry; a" >&2
    echo "       PASS there means the source still says it and the binary is the problem — rebuild:" >&2
  fi
  rebuild_hint "${noliteral_names[@]}"
  echo "       LS_SM_ALLOW_STALE_BINARIES does NOT bypass this axis. A binary pinned on purpose is" >&2
  echo "       still pinned to code containing its registered guard, so nothing legitimate needs" >&2
  echo "       that escape — and \`touch target/debug/*\`, the cheapest answer to a false-stale, is" >&2
  echo "       exactly what makes this the only axis left that can see the truth." >&2
  refused=1
fi
(( refused )) && exit 64

set -a; . "$ENV_CALENDAR"; set +a
[[ -n "${LS_KRX_APPKEY:-}" && -n "${LS_KASI_SERVICE_KEY:-}" ]] || {
  echo "error: .env.calendar did not export both credentials" >&2; exit 64; }
say "credentials loaded from .env.calendar (values never printed)"

# The symbol list is READ from the checkpoint, never hand-authored: catalog membership is an
# input to select_universe, so an unbounded accumulate would re-compose the head's universe.
SYMS="$(python3 -c "
import json
w=json.load(open('$CKPT'))['watermarks']
print(','.join(sorted({k.split('.')[0] for k in w if k.endswith('|1-DAY')})))")"
N_SYMS="$(awk -F, '{print NF}' <<<"$SYMS")"
say "daily symbols from checkpoint: $N_SYMS"
say "watermarks now: $(python3 -c "
import json,collections
w=json.load(open('$CKPT'))['watermarks']
print(dict(collections.Counter(v for k,v in w.items() if k.endswith('|1-DAY'))))")"

# The step [3] fetch window START, derived from the DAILY WATERMARK FRONTIER — never from the
# session date. window.from seeds the KRX witness fetch (fetch_state seeds krx_cursor from it
# and runs to krx_through), and the ingest's accumulate plan acts only on the ESTABLISHED
# PREFIX: it stops before the first calendar-Unknown day and never crosses it
# (established_prefix in src/ingest/mod.rs; enforced_later_session_does_not_cross_the_first_unknown
# and enforced_range_with_intervening_unknown_stops_and_preserves_state in tests/ingest.rs).
# So after a MISSED morning the gap spans 2+ sessions, and a session-date window witnesses only
# the LATEST day — every intermediate day stays Unknown and the bounded ingest stalls at the
# frontier with nothing advanced. Every civil day in (frontier, session] must be establishable,
# which makes the frontier itself the only correct seed:
#   window.from = min(daily watermark) + 1 day, clamped to the session date.
# The MIN governs because each symbol's plan starts at ITS watermark+1 — the slowest symbol
# decides how far back the witness fetch must reach. The clamp keeps the window non-inverted
# once the catalog is already at (or past) the session, which restores the designed
# one-session-per-morning window exactly.
window_from="$(python3 -c "
import json,sys,datetime
try:
    w=json.load(open(sys.argv[1]))['watermarks']
except Exception:
    sys.exit(1)
wms=sorted(v for k,v in w.items() if k.endswith('|1-DAY'))
if not wms:
    sys.exit(1)
frontier=datetime.datetime.strptime(wms[0],'%Y%m%d').date()
session=datetime.datetime.strptime(sys.argv[2],'%Y%m%d').date()
print(min(frontier+datetime.timedelta(days=1),session).isoformat())" \
  "$CKPT" "$session_compact")" \
  || die "could not derive the fetch window start from $CKPT — an unreadable or empty
  watermark set leaves no honest window.from; fix the checkpoint before refreshing."
say "fetch window: $window_from..$session_date (daily frontier + 1, clamped to the session)"

# Keyed on the RUN DATE + BOTH WINDOW ENDS. calendar-fetch-inputs refuses to resume a
# checkpoint whose (window.from, window.through, krx_through) triple differs from the run's
# (CheckpointMismatch: 'start a fresh state file instead'), and BOTH ends now move within a
# day: a completed ingest advances the frontier (later window.from on the documented same-day
# recovery re-run), and a morning run that dies mid-fetch followed by a same-day catch-up
# targeting the NEXT session changes the end while the start stays put. A key missing either
# end hands one of those re-runs a stale checkpoint and dies at step [3]. The through-date
# covers krx_through too — step [3] always passes the session date as both. With the full
# window in the name, every distinct window gets its own state file: an interrupted fetch
# (same window) still resumes; any changed window starts fresh, as the binary requires.
FETCH_CKPT="$STATE/refresh-$(date +%Y%m%d)-from${window_from//-/}-to${session_compact}.calendar-fetch.ckpt"

# ------------------------------------------------------------------------- --dry-run
if (( dry_run )); then
  step "resolved command sequence (dry run — no traffic issued)"
  # The [7]/[8] text below is mode-dependent, and this heredoc is a hand-maintained SECOND COPY
  # of the live path — the duplication that shipped step [3] without --window. Resolving both
  # variants here rather than describing "one or the other" keeps the printed sequence a
  # transcript of what THIS invocation would actually do.
  if (( catch_up )); then
    pace_line="--catch-up: the kill is DISABLED. Progress is still reported every ${poll_secs}s;
           the ingest runs to completion however long it takes."
    gate_line="[8] universe step REFUSED by --catch-up — unconditionally, NOT on the clock.
    The mount is already conceded; the catalog is the deliverable. Falls through to [9]."
    tail_line="[10] NOT RUN — resolving a mount universe is what --catch-up declined to do.

[11] CATCH-UP COMPLETE report, then STOP with exit 41 (a success: catalog advanced,
     no universe in hand, nothing left to retry)."
  else
    pace_line="stand down (kill the ingest, clear the lock, exit 40) as soon as the projected
           finish passes $ingest_by — a universe that lands late takes ZERO trades."
    gate_line="[8] pace gate  (ingest by $ingest_by, universe by $universe_by, opening range 09:15)
    stand down with minutes-remaining rather than resolve a universe that lands too late"
    tail_line="[10] resolve the mount universe  (only after 09:00 — before the auction t8407 serves the
     PREVIOUS session, whose open is a valid positive integer, so the producer would
     silently resolve yesterday's opens)
    env: LS_DATA_HOME=$DATA_HOME
         LS_MOUNT_UNIVERSE_DATE=$mount_date
         LS_MOUNT_UNIVERSE_METADATA=$UNIVERSE_METADATA
         LS_DISPATCH_LANE_ENV=$LANE_ENV
         LS_CALENDAR_SNAPSHOT=$SNAPSHOT
    $BIN/lab-mount-universe --out $OUT_UNIVERSE

[11] GO/NO-GO report, then STOP. --mount is the operator's."
  fi
  cat <<DRY
[1] witness state
    read   $WITNESS_LOG
    probe  curl -H 'AUTH_KEY: \$LS_KRX_APPKEY' \\
             'https://data-dbg.krx.co.kr/svc/apis/sto/stk_bydd_trd?basDd=$session_compact'

[2] archive the active calendar (copy, never move)
    cp $SNAPSHOT $ARCHIVE
    cmp $SNAPSHOT $ARCHIVE

[3] fetch witness inputs  (window START = daily frontier + 1, clamped — covers a
    multi-session gap after a missed morning; the session date alone would leave
    intermediate days Unknown and stall the bounded ingest at the frontier)
    $BIN/calendar-fetch-inputs \\
      --window $window_from..$session_date \\
      --krx-through $session_date \\
      --inputs-out $INPUTS \\
      --state $FETCH_CKPT \\
      --state-root $STATE \\
      --pace-ms 500
    env: LS_CALENDAR_HTTP_TIMEOUT_SECS=180  LS_KRX_APPKEY=<env>  LS_KASI_SERVICE_KEY=<env>

[4] refresh -> candidate
    $BIN/calendar-refresh \\
      --active $SNAPSHOT \\
      --as-of <now UTC RFC3339> \\
      --mode incremental \\
      --through $session_date \\
      --inputs $INPUTS
    writes: $CANDIDATE
            $CANDIDATE_DIFF

[5] assert the diff, then approve + activate
    gate: partial==false, 0 high-risk on the diff, 0 alerts on the CANDIDATE,
          and a status_established entry for $session_date
    write $APPROVAL   (operator=$operator, reviewed_artifact_id copied from the candidate)
    $BIN/calendar-activate \\
      --active $SNAPSHOT --candidate $CANDIDATE \\
      --approval $APPROVAL --as-of <now UTC RFC3339>

[6] verify activation
    $BIN/calendar-status --as-of <now UTC RFC3339> \\
      --snapshot $SNAPSHOT --day $session_date

[7] bounded catch-up ingest ($N_SYMS daily symbols)
    env: LS_TRADING_ENV=paper
         LS_INGEST_LANE_FILE=$LANE_ENV
         LS_CALENDAR_SNAPSHOT=$SNAPSHOT
         LS_SPEND_LEDGER_FILE=$DATA_HOME/state/spend-ledger.json
         LS_INGEST_CATALOG=$CATALOG
         LS_NODE_LOCK_DIR=$CATALOG
         LS_INGEST_KIND=daily  LS_INGEST_MODE=accumulate
         LS_INGEST_SKIP_UNIVERSE_LOAD=1  LS_INGEST_LOOKBACK=$lookback
         LS_INGEST_SYMBOLS=<$N_SYMS symbols read from the checkpoint>
    $BIN/ls-ingest
    watch: poll $CKPT every ${poll_secs}s until all daily watermarks read $session_compact
           APPEND REFUSED  => STOP and report (the rollback workaround is retired)
    pace:  $pace_line

$gate_line

[9] catalog status  (watermark-gated; NO LS_STATUS_* — an expected range asserts one span
    across every bar kind, and the frozen 1-MINUTE series would force NO-GO)
    env: LS_DATA_HOME=$DATA_HOME  LS_CALENDAR_SNAPSHOT=$SNAPSHOT
    $BIN/lab-research catalog status

$tail_line
DRY

  step "self-check: no mount-class command in the resolved sequence"
  # Guard the property structurally rather than trusting the author. Every binary this
  # script runs is invoked as "$BIN/<name>", so that pattern IS the set of invocations --
  # scanning it skips comments and heredoc prose, which merely NAME the forbidden commands
  # in order to say they are the operator's. Two assertions:
  #   (a) lab-live is never invoked. --mount / --dispatch / --genesis / --clear-killswitch
  #       are all lab-live flags, so never running lab-live makes them unreachable.
  #   (b) no invocation line carries a mount-class flag anyway.
  self_path="${BASH_SOURCE[0]}"
  invocations="$(grep -nE '"\$BIN/[a-z-]+"' "$self_path" | grep -vE '^[0-9]+:[[:space:]]*#')"
  bad=0
  if grep -qE '"\$BIN/lab-live"' <<<"$invocations"; then
    echo "  FORBIDDEN: lab-live is invoked"; bad=1
  else
    echo "  ok: lab-live is never invoked -- every mount-class flag is unreachable"
  fi
  for tok in --mount --dispatch --genesis --clear-killswitch --reregister; do
    if grep -qE -- "$tok([^a-zA-Z-]|$)" <<<"$invocations"; then
      echo "  FORBIDDEN: $tok on an invocation line"; bad=1
    fi
  done
  (( bad )) || echo "  ok: no --mount / --dispatch / --genesis / --clear-killswitch / --reregister on any invocation"
  echo "  binaries this script actually invokes:"
  sed -E 's/.*"\$BIN\/([a-z-]+)".*/\1/' <<<"$invocations" | sort -u | sed 's/^/    /'
  echo "  override fields authored by this script: none (operator/reason/citation are operator-only)"
  (( bad )) && exit 64
  exit 0
fi

# ============================================================== live run starts here
step "[1] witness state for $session_date"
witness_rows=""
if [[ -r "$WITNESS_LOG" ]] && grep -q "basDd=$session_compact .*verdict=POSITIVE" "$WITNESS_LOG"; then
  witness_rows="$(grep "basDd=$session_compact .*verdict=POSITIVE" "$WITNESS_LOG" | tail -1 | sed -E 's/.*rows=([0-9]+).*/\1/')"
  say "overnight watcher already recorded POSITIVE ($witness_rows rows) — no probe needed"
else
  say "no positive in the watcher log; probing once"
  # Pass the date EXPLICITLY. Without it the watcher falls back to its own default and the
  # gate that decides whether the catalog may advance gets answered about a different day.
  out="$(LS_WITNESS_DATE="$session_compact" "$script_dir/krx-witness-watch.sh" --once 2>&1)"; rc=$?
  say "$out"
  grep -q "basDd=$session_compact" <<<"$out" \
    || die "the witness probe answered about a different date than $session_compact — refusing
  to read it as this session's publication state."
  case $rc in
    0)  witness_rows="$(sed -E 's/.*rows=([0-9]+).*/\1/' <<<"$out")" ;;
    10) echo
        echo "STAND DOWN: KRX has not published the $session_date witness (clean negative)."
        echo "The catalog CANNOT advance into an unwitnessed session. Do not refresh."
        echo "Go to RUNBOOK-session-morning.md Step 5 — the fidelity decision. The session is"
        echo "not blocked: eligibility is staleness (<=10 days), not same-day currency, so the"
        echo "head resolves off an older prior at reduced fidelity. That is the operator's call."
        exit 40 ;;
    20) die "KRX auth rejected — fix LS_KRX_APPKEY in .env.calendar. This is NOT an unpublished witness." ;;
    *)  die "KRX probe degraded (rc=$rc). Endpoint is slow, not down — retry before standing down." ;;
  esac
fi
say "witness present: $witness_rows rows"

step "[2] archive the active calendar"
# Never clobber an existing archive. The name is keyed on today's date, and a re-run is the
# documented recovery after a stand-down — so an unconditional copy would overwrite the
# PRE-refresh snapshot with the already-activated one, destroying the only rollback target
# at exactly the moment it is most likely to be needed.
if [[ -e "$ARCHIVE" ]]; then
  say "archive already exists from an earlier run today — keeping it (it is the pre-refresh state)"
  say "archive: $ARCHIVE"
else
  cp "$SNAPSHOT" "$ARCHIVE" || die "archive copy failed"
  cmp -s "$SNAPSHOT" "$ARCHIVE" || die "archive verify failed — do not proceed without a rollback target"
  say "archive verified: $ARCHIVE"
fi

step "[3] fetch witness inputs"
# --window is REQUIRED and is NOT a synonym for --krx-through: it supplies the START of the
# witness fetch (fetch_state seeds krx_cursor from window.from and runs it to krx_through), and
# the KASI year span. --krx-through alone leaves the range with no start and the binary refuses
# before any network call. The window START is $window_from — the daily watermark frontier + 1,
# clamped to the session date (derived above) — NOT the session date: after a MISSED morning
# the gap spans 2+ sessions, and a session-date window witnesses only the latest day while the
# ingest stalls at the first Unknown intermediate day. On the designed one-session-per-morning
# cadence the frontier is yesterday's session, so the clamp reduces this to the session date
# and the fetch cost is identical to the old single-day window.
# --state-root is REQUIRED here even though every path below is absolute. The binary confines
# all output beneath an owner-local state root that DEFAULTS TO "state" RELATIVE TO CWD, and
# confine() runs before any network call — so without it the absolute --inputs-out resolves
# outside the root and the run is refused from every directory except adapters/nautilus. This
# script is otherwise CWD-independent by construction; passing $STATE keeps step [3] that way.
LS_CALENDAR_HTTP_TIMEOUT_SECS="${LS_CALENDAR_HTTP_TIMEOUT_SECS:-180}" \
  "$BIN/calendar-fetch-inputs" \
    --window "$window_from..$session_date" \
    --krx-through "$session_date" \
    --inputs-out "$INPUTS" \
    --state "$FETCH_CKPT" \
    --state-root "$STATE" \
    --pace-ms 500 || die "calendar-fetch-inputs failed — READ ITS ERROR ABOVE, printed by the
  binary itself, rather than assuming a cause. Three failure modes look nothing alike:
    * 'error: missing required/unknown argument ...' is an ARGUMENT defect in this script,
      reached in seconds and before any network call. Raising a timeout cannot help.
    * 'refused: <path> resolves outside the owner-local state root <root>' is a CONFINEMENT
      defect — --state-root disagrees with the output paths (or is missing, and the root
      defaulted to \$PWD/state). Also pre-network; a timeout is equally irrelevant.
    * 'failed=error sending request' / 'client-side timeout' IS the client-side timeout trap,
      not a dead source — raise LS_CALENDAR_HTTP_TIMEOUT_SECS and re-run; the checkpoint
      resumes so only un-fetched days cost anything."
say "inputs: $INPUTS"

step "[4] refresh -> candidate"
AS_OF="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
"$BIN/calendar-refresh" \
  --active "$SNAPSHOT" --as-of "$AS_OF" \
  --mode incremental --through "$session_date" \
  --inputs "$INPUTS" || die "calendar-refresh failed"

step "[5] gate the diff, then approve + activate"
[[ -r "$CANDIDATE_DIFF" ]] || die "no candidate diff at $CANDIDATE_DIFF"
[[ -r "$CANDIDATE" ]] || die "no candidate snapshot at $CANDIDATE"
# Values reach python through argv, never string interpolation into its source.
#
# Two corrections the review caught, both the same class of bug — a gate asserting something
# it never actually checked, and then an approval artifact attesting that assertion as
# reviewed fact:
#   * `alerts` lives on the CANDIDATE SNAPSHOT, not on the diff. Reading it from the diff
#     always yielded [], so the "0 alerts" assertion could never fail.
#   * "established as trading_session" was tested as "some entry mentions this date", any
#     category. A diff touching the date for an unrelated reason satisfied it.
gate="$(python3 -c "
import json,sys
diff_path, cand_path, session_date = sys.argv[1], sys.argv[2], sys.argv[3]
d=json.load(open(diff_path))
entries=d.get('entries') or d.get('diff') or []
partial=bool(d.get('partial'))
high=[e for e in entries if str(e.get('risk','')).lower()=='high' or e.get('high_risk')]
alerts=json.load(open(cand_path)).get('alerts') or []
established=[e for e in entries
             if e.get('date')==session_date and e.get('category')=='status_established']
print(json.dumps({'entries':len(entries),'partial':partial,'high':len(high),
                  'alerts':len(alerts),'target':len(established)}))" \
  "$CANDIDATE_DIFF" "$CANDIDATE" "$session_date" 2>/dev/null)" \
  || die "could not parse $CANDIDATE_DIFF / $CANDIDATE — review them by hand"
say "diff: $gate"
python3 -c "
import json,sys
g=json.loads(sys.argv[1]); session_date=sys.argv[2]
fail=[]
if g['partial']: fail.append('partial=true — partial:source-failure is ACKNOWLEDGEABLE, and acknowledging it consumes the chain transition while leaving every consumer as blocked as before')
if g['high']:   fail.append('%d high-risk entr(y/ies)' % g['high'])
if g['alerts']: fail.append('%d alert(s) on the candidate' % g['alerts'])
if not g['target']: fail.append('no status_established entry for %s — the witness did not certify it as a trading session' % session_date)
if fail:
    print('DIFF GATE FAILED:'); [print('  - '+f) for f in fail]; sys.exit(1)
print('diff gate passed')" "$gate" "$session_date" \
  || die "diff gate failed — keep the candidate and diff for review, do not approve"

if (( stop_before_activate )); then
  step "stopping before activation as requested"
  say "candidate: $CANDIDATE"; say "diff:      $CANDIDATE_DIFF"
  exit 0
fi

CAND_ID="$(python3 -c "import json;print(json.load(open('$CANDIDATE'))['artifact_id'])")" \
  || die "could not read artifact_id from the candidate"
# The approval's reason is GENERATED FROM THE GATE RESULT, never hardcoded — an approval
# that recites checks the gate did not actually perform is how the vacuous `alerts`
# assertion above went unnoticed. Values ride argv, not interpolated python source.
python3 -c "
import json,sys
operator, session_date, as_of, cand_id, out_path, gate_json = sys.argv[1:7]
g=json.loads(gate_json)
json.dump({
 'operator':operator,
 'reason':(f'Incremental refresh through {session_date}: establishes the KRX witness so it '
           f'reads trading_session, unblocking the bounded catch-up ingest. Diff gated '
           f'automatically against the candidate: partial={g[\"partial\"]}, '
           f'{g[\"high\"]} high-risk of {g[\"entries\"]} entries, {g[\"alerts\"]} candidate '
           f'alert(s), {g[\"target\"]} status_established entr(y/ies) for {session_date}.'),
 'approved_at':as_of,
 'reviewed_artifact_id':cand_id,
 'acknowledged':[]}, open(out_path,'w'), indent=2)" \
  "$operator" "$session_date" "$AS_OF" "$CAND_ID" "$APPROVAL" "$gate"
say "approval: $APPROVAL (binds candidate $CAND_ID)"

# Activation CHANGES artifact_id, and the operator's Unknown override binds to the in-force
# identity — so it must land BEFORE they author the override, never after.
"$BIN/calendar-activate" \
  --active "$SNAPSHOT" --candidate "$CANDIDATE" \
  --approval "$APPROVAL" --as-of "$AS_OF" || die "calendar-activate failed"

step "[6] verify activation"
"$BIN/calendar-status" --as-of "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --snapshot "$SNAPSHOT" --day "$session_date" || die "calendar-status failed"
NEW_ID="$(python3 -c "import json;print(json.load(open('$SNAPSHOT'))['artifact_id'])" 2>/dev/null)"
say "in-force artifact_id: $NEW_ID   <-- the operator needs this VERBATIM for the override"

step "[7] bounded catch-up ingest ($N_SYMS symbols -> $session_compact)"
# mktemp substitutes only a TRAILING run of X's — a template ending in `.log` has none, so
# BSD/macOS mktemp errors and returns empty. An empty path would send the ingest's output to
# a failed redirect and leave the APPEND REFUSED grep reading a file that does not exist,
# blinding the one terminal condition this chain must stop on.
ingest_log="$(mktemp "${TMPDIR:-/tmp}/session-morning-ingest.XXXXXX")" \
  && [[ -n "$ingest_log" ]] || die "could not create the ingest log (mktemp failed)"
start_epoch="$(date +%s)"
advanced_at_start="$(count_advanced)"
[[ "$advanced_at_start" == "-1" ]] && die "could not read $CKPT to establish the ingest baseline"
say "already at or past $session_compact: $advanced_at_start/$N_SYMS"

( LS_TRADING_ENV=paper \
  LS_INGEST_LANE_FILE="$LANE_ENV" \
  LS_CALENDAR_SNAPSHOT="$SNAPSHOT" \
  LS_SPEND_LEDGER_FILE="$DATA_HOME/state/spend-ledger.json" \
  LS_INGEST_CATALOG="$CATALOG" \
  LS_NODE_LOCK_DIR="$CATALOG" \
  LS_INGEST_KIND=daily LS_INGEST_MODE=accumulate \
  LS_INGEST_SKIP_UNIVERSE_LOAD=1 LS_INGEST_LOOKBACK="$lookback" \
  LS_INGEST_SYMBOLS="$SYMS" \
  "$BIN/ls-ingest" >"$ingest_log" 2>&1 ) &
ingest_pid=$!

# ls-ingest prints nothing useful and ignores RUST_LOG, so progress is READ FROM THE
# CHECKPOINT, never from the log or the exit code (`exit 0 / 0 bars` is the signature of a
# fully-blocked run AND a fully-up-to-date one). The checkpoint also does not advance while
# a symbol is mid-throttle, which is why the pace check projects from observed throughput.
ing_dl="$(hhmm_epoch "$ingest_by")"
# NOTE: there is deliberately no in-loop APPEND REFUSED check. ls-ingest emits its refusal
# report only at the END of the run, so a mid-run grep can never fire while the process
# lives — it was dead code. The authoritative check is after `wait` below, and every path
# that kills the ingest early re-scans the log so a refusal already written is still
# reported rather than discarded along with the process.
while kill -0 "$ingest_pid" 2>/dev/null; do
  sleep "$poll_secs"
  adv="$(count_advanced)"
  [[ "$adv" == "-1" || -z "$adv" ]] && continue    # checkpoint mid-write; skip this poll
  gained=$((adv - advanced_at_start))
  elapsed=$(( $(date +%s) - start_epoch ))
  # MODE LOGIC LIVES HERE, NOT IN pace_verdict. The evaluator stays a pure function of
  # (advanced, total, elapsed, now, deadline) so --self-test can keep driving it with a
  # simulated clock; deciding whether its verdict is ACTIONABLE is the caller's job.
  #
  # On a catch-up the deadline is meaningless by construction: LS_SM_INGEST_BY resolves as
  # TODAY at 09:05, so on a weekend run at any normal hour it is already elapsed and the first
  # poll returns LATE — with gained=0 through the `now >= dl` branch, and with gained>0 because
  # any projected finish is past an elapsed deadline. Reporting that verdict would be noise at
  # best and an invitation to kill a healthy ingest at worst, so it is not computed at all.
  if (( catch_up )); then
    say "ingest $adv/$N_SYMS at $session_compact | elapsed ${elapsed}s | pace gate OFF (--catch-up)"
    continue
  fi
  read -r verdict finish remain eta <<<"$(pace_verdict "$gained" "$((N_SYMS - advanced_at_start))" "$elapsed" "$(now_epoch)" "$ing_dl")"
  say "ingest $adv/$N_SYMS at $session_compact | elapsed ${elapsed}s | projected $finish | $verdict"
  if [[ "$verdict" == "LATE" ]]; then
    kill "$ingest_pid" 2>/dev/null; wait "$ingest_pid" 2>/dev/null
    clear_ingest_lock
    # The kill may have pre-empted ls-ingest's end-of-run refusal report, but if a refusal
    # was already written it must not vanish with the process — a refusal outranks the
    # pace verdict as the thing the operator needs to hear.
    if grep -q "APPEND REFUSED" "$ingest_log" 2>/dev/null; then
      echo; sed 's/^/  | /' "$ingest_log" | tail -20
      die "APPEND REFUSED (found while standing down) — STOP. The watermark-rollback
  workaround is RETIRED; a refusal now means a NEW problem. Report state, do not retry."
    fi
    step "STAND DOWN — not on pace"
    cat <<STANDDOWN
The ingest is projected to finish at $finish, past the $ingest_by target.
  progress            $adv/$N_SYMS symbols at $session_compact
  minutes to $ingest_by      $remain
  projected ETA       $eta more minutes
Resolving a universe now would hand the operator a file that lands after 09:10, and head v34
builds its opening range only from bars observed 09:00-09:15 — a late mount takes ZERO trades.
Standing down here is the SUCCESS path, not a failure. On a paper lane it costs the rehearsal
only: is_clean_session gates on trading_env == "live" (ladder.rs:348), so no rung-1 clean
session is consumed either way.
No universe resolved. The catalog keeps whatever the ingest committed, and the ingest lock
has been cleared, so re-running is safe.
STANDDOWN
    exit 40
  fi
done
wait "$ingest_pid"; ing_rc=$?
say "ls-ingest exited rc=$ing_rc in $(( $(date +%s) - start_epoch ))s"
if grep -q "APPEND REFUSED" "$ingest_log" 2>/dev/null; then
  clear_ingest_lock
  echo; sed 's/^/  | /' "$ingest_log" | tail -20
  die "APPEND REFUSED — STOP. The watermark-rollback workaround is RETIRED; a refusal now
  means a NEW problem (PR #228 trims collect_daily rows to the requested window and was
  validated live on 2026-07-29 at 75/75 clean). Report state, do not retry, do not roll back."
fi

final_adv="$(count_advanced)"
[[ "$final_adv" == "-1" ]] && die "could not read $CKPT after the ingest — verify with the
  watermark by hand before proceeding; an unreadable checkpoint is not a clean run."
say "watermarks at or past $session_compact: $final_adv/$N_SYMS"
if (( final_adv < N_SYMS )); then
  clear_ingest_lock
  die "partial ingest ($final_adv/$N_SYMS). A mixed watermark distribution is a partial run —
  resume, do not proceed. Verify with the watermark, never the exit code."
fi

step "[8] pace gate before the universe step"
# The universe refusal is the half of the pace machinery --catch-up KEEPS. It is refused
# UNCONDITIONALLY here, never on the clock: a catch-up run started at 08:00 on a Saturday would
# pass the $universe_by test and go resolve a universe for a day KRX is closed. The mount was
# conceded when the caller passed --catch-up; the clock has no say in it.
if (( catch_up )); then
  say "--catch-up: the universe step is refused BY DESIGN, not on the clock."
  say "the catalog is the deliverable — running [9] to certify it, then stopping."
else
  uni_dl="$(hhmm_epoch "$universe_by")"
  now="$(now_epoch)"
  if (( now >= uni_dl )); then
    step "STAND DOWN — past the $universe_by universe deadline"
    echo "The ingest completed, but the universe would land after $universe_by and the opening"
    echo "range opens at 09:15. Not resolving. Paper lane, so no clean session is consumed."
    exit 40
  fi
  say "$(( (uni_dl - now) / 60 )) min to $universe_by — proceeding"
fi

step "[9] catalog status"
# Watermark-gated, NOT bounded. LS_STATUS_SDATE/EDATE would assert one span across every
# (instrument, bar-kind) series; the 1-MINUTE series are frozen weeks behind the daily ones, so a
# daily-derived range forces NO-GO whatever the daily frontier looks like.
LS_DATA_HOME="$DATA_HOME" LS_CALENDAR_SNAPSHOT="$SNAPSHOT" \
  "$BIN/lab-research" catalog status || say "WARNING: catalog status returned non-zero — read its verdict below"

# The catch-up run ENDS here, one step short of the universe. Everything below this point exists
# to hand an operator a mountable universe, which a catch-up has already declined to produce.
if (( catch_up )); then
  step "CATCH-UP COMPLETE"
  cat <<CATCHUP
The calendar advanced through $session_date and the catalog is at $final_adv/$N_SYMS daily
watermarks for $session_compact. Read the catalog status verdict above — it is the deliverable.
  in-force artifact_id  $NEW_ID
  no universe resolved  BY DESIGN (--catch-up); the mount was conceded before the run started
Exit 41 says exactly that: a complete catch-up, NOT a stand-down (40) and NOT a GO (0) — there
is no universe file to mount, and there is nothing left to retry.
CATCHUP
  exit 41
fi

step "[10] resolve the mount universe for $mount_date"
# The REAL clock, never LS_SM_NOW. That override exists for pace-check testing and must not
# reach a network call: the guard below is the only thing standing between this script and
# a pre-auction t8407 read, which returns the PREVIOUS session's snapshot with a perfectly
# valid positive open. A forgotten `export LS_SM_NOW=09:30` from a pace test would otherwise
# silently resolve yesterday's opens into today's mount universe.
hhmm="$(date +%H%M)"
if (( 10#$hhmm < 900 )); then
  die "refusing to resolve before 09:00 — before the auction t8407 serves the PREVIOUS
  session's snapshot, whose open is a valid positive integer, so the producer would silently
  resolve yesterday's opens."
fi
uni_log="$(mktemp "${TMPDIR:-/tmp}/session-morning-universe.XXXXXX")" \
  && [[ -n "$uni_log" ]] || die "could not create the universe log (mktemp failed)"
LS_DATA_HOME="$DATA_HOME" \
LS_MOUNT_UNIVERSE_DATE="$mount_date" \
LS_MOUNT_UNIVERSE_METADATA="$UNIVERSE_METADATA" \
LS_DISPATCH_LANE_ENV="$LANE_ENV" \
LS_CALENDAR_SNAPSHOT="$SNAPSHOT" \
  "$BIN/lab-mount-universe" --out "$OUT_UNIVERSE" 2>&1 | tee "$uni_log"
uni_rc="${PIPESTATUS[0]}"

step "[11] GO / NO-GO"
if (( uni_rc == 0 )) && [[ -s "$OUT_UNIVERSE" ]]; then
  python3 -c "
import json
rows=json.load(open('$OUT_UNIVERSE'))
rows=rows.get('rows',rows) if isinstance(rows,dict) else rows
gaps=[abs(float(r.get('gap_pct',r.get('gap',0)))) for r in rows] if rows else []
print(f'  universe: {len(rows)} symbols')
if gaps: print(f'  gap range: {min(gaps):.2f}% .. {max(gaps):.2f}%')" 2>/dev/null || say "universe written"
  echo "  file: $OUT_UNIVERSE"
  echo
  echo "GO. Operator checklist:"
  echo "  artifact_id (verbatim, for the Unknown override): $NEW_ID"
  echo "  LS_MOUNT_UNIVERSE_FILE=$OUT_UNIVERSE"
  echo "  catalog_watermark: check --dispatch's own output before deferring anything."
  echo "     Whether a deferral is needed depends on which binaries you are running. Binaries"
  echo "     built from post-#231 main evaluate the check against the REAL catalog, so a clean"
  echo "     ingest should go green and NO deferral should be pre-planned — a red there is a"
  echo "     genuine finding worth reading, not routine. Older binaries compute it only from"
  echo "     the LS_DISPATCH_STUB_CATALOG stub and red unconditionally; only those need"
  echo "     LS_DISPATCH_DEFER=catalog_watermark. Either way NEVER set the stub to 'ok' — that"
  echo "     asserts a check rather than evaluating it. Deferrals accumulate (k_window 5,"
  echo "     max_deferrals 3), which is why a blanket defer is not free."
  echo "  minutes to 09:15: $(( ($(hhmm_epoch 09:15) - $(now_epoch)) / 60 ))"
  echo
  echo "STOPPING HERE. --dispatch and --mount are nonce-gated, attended, and the operator's."
  echo "This script never authors the override's operator / reason / citation."
  rm -f "$uni_log"
  exit 0
fi

# A non-zero rc is NOT automatically GO. Exactly one non-zero outcome is a success: the
# producer refusing because every overnight gap fell under the floor (a genuine flat open).
# Everything else — a crash, a missing env var, a gateway auth failure — must exit NO-GO,
# because exit 0 is this script's GO code and a wrapper reading it would be told a chain
# that produced nothing had succeeded, at 09:10, with minutes left before the opening range.
if grep -qiE 'flat open|every gap|gap floor|below the gap|no candidates' "$uni_log"; then
  echo "  lab-mount-universe rc=$uni_rc — VALID FLAT-OPEN refusal (every gap under the floor)."
  echo "  Report 'no mount today'. This is a correct head outcome, not an error"
  echo "  (2026-07-28 was exactly this)."
  rm -f "$uni_log"
  exit 0
fi
echo "  lab-mount-universe rc=$uni_rc, no universe written, and the output does not match the"
echo "  flat-open refusal signature. Treating as NO-GO rather than guessing."
echo "  Last output:"
sed 's/^/    | /' "$uni_log" | tail -20
rm -f "$uni_log"
exit 1
