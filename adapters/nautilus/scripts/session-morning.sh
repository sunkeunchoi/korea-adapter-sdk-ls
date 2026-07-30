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
# WHAT THIS SCRIPT WILL NEVER DO. It never runs `--mount`, `--dispatch`, or `--genesis`,
# and it never authors the attended Unknown override's `operator`, `reason`, or `citation`
# fields. Those are operator-only, nonce-gated, and TTY-gated by design. This script stops
# at a GO/NO-GO report and hands the operator a checklist.
#
# Usage:
#   ./session-morning.sh --dry-run              # print the resolved sequence, zero traffic
#   ./session-morning.sh --self-test            # exercise the pace check, zero traffic
#   ./session-morning.sh                        # run it
#   ./session-morning.sh --stop-before-activate # stop after the diff for a manual review
#
# Env (all optional — every default is resolved below and printed by --dry-run):
#   LS_SM_SESSION_DATE  session to ingest, the PREVIOUS session      (default 2026-07-29)
#   LS_SM_MOUNT_DATE    session to resolve a universe FOR, today     (default 2026-07-30)
#   LS_SM_INGEST_BY     ingest-completion target HH:MM local         (default 09:05)
#   LS_SM_UNIVERSE_BY   universe-in-hand target HH:MM local          (default 09:10)
#   LS_SM_NOW           override "now" as HH:MM — pace testing ONLY  (default: real clock)
#   LS_SM_OPERATOR      operator id written into the calendar approval (default sunkeunchoi)
#   LS_SM_LOOKBACK      ingest coverage floor YYYYMMDD               (default 20260518)
#
# Exit codes (the contract — never read success from log text):
#   0   GO      — universe resolved (or a valid flat-open refusal), report delivered
#   1   NO-GO   — a step failed in a way the runbook anticipates; state reported
#   40  STAND-DOWN — not on pace; reported early and deliberately, before the universe step
#   64  misconfiguration — refused before issuing any traffic
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
FETCH_CKPT="$STATE/refresh-$(date +%Y%m%d).calendar-fetch.ckpt"
CANDIDATE="$SNAPSHOT.candidate"
CANDIDATE_DIFF="$SNAPSHOT.candidate.diff.json"
ARCHIVE="$SNAPSHOT.archive-$(date +%Y%m%d)"

dry_run=0; self_test=0; stop_before_activate=0
for a in "$@"; do case "$a" in
  --dry-run) dry_run=1 ;;
  --self-test) self_test=1 ;;
  --stop-before-activate) stop_before_activate=1 ;;
  *) echo "error: unknown argument '$a'" >&2; exit 64 ;;
esac; done

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
missing=0
for f in "$BIN/calendar-fetch-inputs" "$BIN/calendar-refresh" "$BIN/calendar-activate" \
         "$BIN/calendar-status" "$BIN/ls-ingest" "$BIN/lab-research" "$BIN/lab-mount-universe" \
         "$SNAPSHOT" "$LANE_ENV" "$ENV_CALENDAR" "$UNIVERSE_METADATA" "$CKPT"; do
  if [[ -e "$f" ]]; then say "ok   $f"; else say "MISS $f"; missing=$((missing+1)); fi
done
(( missing )) && { echo "error: $missing required path(s) missing" >&2; exit 64; }

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

# ------------------------------------------------------------------------- --dry-run
if (( dry_run )); then
  step "resolved command sequence (dry run — no traffic issued)"
  cat <<DRY
[1] witness state
    read   $WITNESS_LOG
    probe  curl -H 'AUTH_KEY: \$LS_KRX_APPKEY' \\
             'https://data-dbg.krx.co.kr/svc/apis/sto/stk_bydd_trd?basDd=$session_compact'

[2] archive the active calendar (copy, never move)
    cp $SNAPSHOT $ARCHIVE
    cmp $SNAPSHOT $ARCHIVE

[3] fetch witness inputs
    $BIN/calendar-fetch-inputs \\
      --krx-through $session_date \\
      --inputs-out $INPUTS \\
      --state $FETCH_CKPT \\
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
    watch: poll $CKPT until all daily watermarks read $session_compact
           APPEND REFUSED  => STOP and report (the rollback workaround is retired)

[8] pace gate  (ingest by $ingest_by, universe by $universe_by, opening range 09:15)
    stand down with minutes-remaining rather than resolve a universe that lands too late

[9] catalog status  (watermark-gated; NO LS_STATUS_* — an expected range asserts one span
    across every bar kind, and the frozen 1-MINUTE series would force NO-GO)
    env: LS_DATA_HOME=$DATA_HOME  LS_CALENDAR_SNAPSHOT=$SNAPSHOT
    $BIN/lab-research catalog status

[10] resolve the mount universe  (only after 09:00 — before the auction t8407 serves the
     PREVIOUS session, whose open is a valid positive integer, so the producer would
     silently resolve yesterday's opens)
    env: LS_DATA_HOME=$DATA_HOME
         LS_MOUNT_UNIVERSE_DATE=$mount_date
         LS_MOUNT_UNIVERSE_METADATA=$UNIVERSE_METADATA
         LS_DISPATCH_LANE_ENV=$LANE_ENV
         LS_CALENDAR_SNAPSHOT=$SNAPSHOT
    $BIN/lab-mount-universe --out $OUT_UNIVERSE

[11] GO/NO-GO report, then STOP. --mount is the operator's.
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
LS_CALENDAR_HTTP_TIMEOUT_SECS="${LS_CALENDAR_HTTP_TIMEOUT_SECS:-180}" \
  "$BIN/calendar-fetch-inputs" \
    --krx-through "$session_date" \
    --inputs-out "$INPUTS" \
    --state "$FETCH_CKPT" \
    --pace-ms 500 || die "calendar-fetch-inputs failed. 'failed=error sending request' is the
  CLIENT-side timeout trap, not a dead source — raise LS_CALENDAR_HTTP_TIMEOUT_SECS and re-run;
  the checkpoint resumes so only un-fetched days cost anything."
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
  sleep 30
  adv="$(count_advanced)"
  [[ "$adv" == "-1" || -z "$adv" ]] && continue    # checkpoint mid-write; skip this poll
  gained=$((adv - advanced_at_start))
  elapsed=$(( $(date +%s) - start_epoch ))
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
uni_dl="$(hhmm_epoch "$universe_by")"
now="$(now_epoch)"
if (( now >= uni_dl )); then
  step "STAND DOWN — past the $universe_by universe deadline"
  echo "The ingest completed, but the universe would land after $universe_by and the opening"
  echo "range opens at 09:15. Not resolving. Paper lane, so no clean session is consumed."
  exit 40
fi
say "$(( (uni_dl - now) / 60 )) min to $universe_by — proceeding"

step "[9] catalog status"
# Watermark-gated, NOT bounded. LS_STATUS_SDATE/EDATE would assert one span across every
# (instrument, bar-kind) series; the 1-MINUTE series are frozen weeks behind the daily ones, so a
# daily-derived range forces NO-GO whatever the daily frontier looks like.
LS_DATA_HOME="$DATA_HOME" LS_CALENDAR_SNAPSHOT="$SNAPSHOT" \
  "$BIN/lab-research" catalog status || say "WARNING: catalog status returned non-zero — read its verdict below"

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
