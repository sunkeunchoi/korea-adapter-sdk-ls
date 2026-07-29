#!/usr/bin/env bash
# krx-witness-watch.sh — U3: unattended overnight watch for a KRX daily witness.
#
# The catalog cannot advance into a session the calendar has not witnessed, and KRX
# publishes a session's daily witness RETROSPECTIVELY — observed on 2026-07-28/29 as
# empty from 17:45 through 07:51, landing somewhere in the 07:51–08:51 window the next
# morning. So the answer to "did it publish?" is unknowable the night before and is
# needed at 08:45 sharp. This script buys that answer in advance: it polls hourly and
# leaves the outcome on disk, so the morning session READS publication state instead of
# spending its own clock discovering it.
#
# Read-only and KRX-only. It issues no LS gateway traffic (R10) and writes nothing but
# its own log — no calendar artifact, no catalog, no state/ (R9).
#
# Usage:
#   ./krx-witness-watch.sh                 # hourly watch for $LS_WITNESS_DATE until positive
#   ./krx-witness-watch.sh --once          # single probe, then exit on the verdict code
#   LS_WITNESS_DATE=20260728 ./krx-witness-watch.sh --once   # positive control
#
# Env:
#   LS_WITNESS_DATE      basDd to probe, YYYYMMDD (default: 20260729)
#   LS_WITNESS_INTERVAL  seconds between attempts (default: 3600)
#   LS_WITNESS_UNTIL     stop after this local time even without a positive, as
#                        YYYY-MM-DDTHH:MM (default: the NEXT 09:30 local — i.e. tomorrow
#                        morning when started this evening). Must be an absolute instant:
#                        an HHMM-only compare reads "10:46 > 09:30" as an expired deadline
#                        and exits after one attempt on any watch started after 09:30.
#   LS_WITNESS_LOG       log path (default: <scriptdir>/krx-witness-watch.log)
#   LS_ENV_CALENDAR      credential file (default: <repo>/.env.calendar)
#
# Exit codes (the contract — never read success from the log text):
#   0   witness published (rows > 0). Polling stopped.
#   10  clean negative — a 200 with zero rows. NOT a failure; the witness does not exist yet.
#   20  auth rejected (401/403). A bad key, NOT an unpublished witness — the whole point of
#       logging these distinctly is that they are indistinguishable in a row count.
#   30  transport/degraded (timeout, empty body, unparseable payload). Endpoint is slow, not down.
#   40  deadline reached with no positive, having seen at least one real answer from KRX.
#   41  deadline reached and EVERY attempt was auth-rejected — the watch proves nothing about
#       publication. Distinct from 40 on purpose: the log already separates a bad key from an
#       unpublished witness, and the exit code must not throw that distinction away for
#       anything reading the code rather than the text.
set -uo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/../../.." && pwd)"

witness_date="${LS_WITNESS_DATE:-20260729}"
interval="${LS_WITNESS_INTERVAL:-3600}"
# Absolute deadline instant, as epoch seconds. Defaults to the next 09:30 local that is
# still in the future, so a watch started at 17:00 today stops at 09:30 tomorrow.
# Both values come back on ONE stdout line — no temp file. The earlier version handed the
# human form out through a fixed /tmp/witness-deadline.$$ path, which is predictable enough
# to be pre-created as a symlink on a shared /tmp, and inconsistent with probe() below,
# which already uses mktemp.
read -r deadline_epoch deadline_human <<<"$(python3 -c "
import datetime,sys
a=sys.argv[1]
if a:
    d=datetime.datetime.strptime(a,'%Y-%m-%dT%H:%M')
else:
    n=datetime.datetime.now()
    d=n.replace(hour=9,minute=30,second=0,microsecond=0)
    if d<=n: d+=datetime.timedelta(days=1)
print(int(d.timestamp()), d.strftime('%Y-%m-%dT%H:%M'))" "${LS_WITNESS_UNTIL:-}" 2>/dev/null)" || true
if [[ -z "${deadline_epoch:-}" || ! "$deadline_epoch" =~ ^[0-9]+$ ]]; then
  echo "error: could not parse LS_WITNESS_UNTIL='${LS_WITNESS_UNTIL:-}' (want YYYY-MM-DDTHH:MM)" >&2
  exit 64
fi
log="${LS_WITNESS_LOG:-$script_dir/krx-witness-watch.log}"
env_calendar="${LS_ENV_CALENDAR:-$repo_root/.env.calendar}"
once=0
[[ "${1:-}" == "--once" ]] && once=1

if [[ ! "$witness_date" =~ ^[0-9]{8}$ ]]; then
  echo "error: LS_WITNESS_DATE must be YYYYMMDD, got '$witness_date'" >&2; exit 64
fi

# Credentials ride in the process env only, sourced from the gitignored 0600 lane file (U1).
# Never hardcoded here, never echoed, never passed as an argument.
if [[ ! -r "$env_calendar" ]]; then
  echo "error: credential file $env_calendar not readable" >&2; exit 64
fi
set -a; . "$env_calendar"; set +a
if [[ -z "${LS_KRX_APPKEY:-}" ]]; then
  echo "error: LS_KRX_APPKEY not set by $env_calendar" >&2; exit 64
fi

log_line() { printf '%s\n' "$*" >>"$log"; printf '%s\n' "$*"; }

# One probe. Echoes "<http> <bytes> <rows>"; rows is -1 when the payload is not countable.
probe() {
  local body http bytes rows meta
  # mktemp substitutes only a TRAILING run of X's, so a `.json` suffix after them leaves the
  # template with none: BSD/macOS mktemp errors and returns empty, curl writes nowhere, and
  # every probe would classify DEGRADED no matter what KRX actually returned — an overnight
  # watcher logging nothing usable. The extension was decorative; json.load does not care.
  body="$(mktemp "${TMPDIR:-/tmp}/krx-witness.XXXXXX")"
  if [[ -z "$body" ]]; then
    echo "000 0 -1"   # mktemp failed -> DEGRADED, explicitly rather than by accident
    return
  fi
  # AUTH_KEY rides a header, not the URL, so it never lands in a redirect or an access log.
  # It IS visible in this host's process table for the life of the call — accepted on a
  # single-user host, and the same exposure the runbook's Step 1 probe already carries.
  meta="$(curl -s -o "$body" -w '%{http_code} %{size_download}' --max-time 240 \
    -H "AUTH_KEY: $LS_KRX_APPKEY" \
    "https://data-dbg.krx.co.kr/svc/apis/sto/stk_bydd_trd?basDd=$witness_date" 2>/dev/null)"
  http="${meta%% *}"; bytes="${meta##* }"
  [[ -z "$http" ]] && http=000
  rows="$(python3 -c "
import json,sys
try:
    d=json.load(open(sys.argv[1]))
except Exception:
    print(-1); sys.exit()
b=d.get('OutBlock_1')
print(len(b) if isinstance(b,list) else -1)" "$body" 2>/dev/null)"
  [[ -z "$rows" ]] && rows=-1
  rm -f "$body"
  echo "$http ${bytes:-0} $rows"
}

# Classify one probe, log it, and return the exit-code contract above.
attempt() {  # $1 = attempt number
  local n="$1" http bytes rows verdict rc ts
  read -r http bytes rows <<<"$(probe)"
  ts="$(date '+%Y-%m-%dT%H:%M:%S%z')"
  if [[ "$http" == "200" && "$rows" -gt 0 ]]; then
    verdict=POSITIVE; rc=0
  elif [[ "$http" == "200" && "$rows" == "0" ]]; then
    # A 200 with an empty OutBlock_1 is a CLEAN NEGATIVE, not an error. No amount of
    # refreshing invents a witness; the only cure is waiting. Keep polling.
    verdict=NEGATIVE; rc=10
  elif [[ "$http" == "401" || "$http" == "403" ]]; then
    verdict=AUTH_REJECTED; rc=20
  else
    # Includes http=000 (client timeout) and a 200 whose body would not parse: the KRX
    # daily endpoint has been observed at 14–59 s/day under load. Degraded, not down.
    verdict=DEGRADED; rc=30
  fi
  log_line "$ts basDd=$witness_date attempt=$n http=$http bytes=$bytes rows=$rows verdict=$verdict"
  return $rc
}

if (( once )); then
  attempt 1; exit $?
fi

log_line "$(date '+%Y-%m-%dT%H:%M:%S%z') === watch start basDd=$witness_date interval=${interval}s until=$deadline_human pid=$$ ==="
n=0
answered=0   # attempts where KRX actually answered about publication (not an auth rejection)
while :; do
  n=$((n+1))
  attempt "$n"; rc=$?
  [[ "$rc" != "20" ]] && answered=$((answered+1))
  case $rc in
    0)  log_line "$(date '+%Y-%m-%dT%H:%M:%S%z') === witness PUBLISHED for $witness_date after $n attempts — stopping ==="
        exit 0 ;;
    20) # Keep watching: a key can be fixed mid-watch, and stopping here would leave the
        # morning with no log at all. The distinct verdict is what stops a bad key from
        # being read as an unpublished witness.
        log_line "$(date '+%Y-%m-%dT%H:%M:%S%z') !!! AUTH REJECTED — check LS_KRX_APPKEY in .env.calendar; this is NOT an unpublished witness" ;;
  esac
  if (( $(date +%s) >= deadline_epoch )); then
    if (( answered == 0 )); then
      log_line "$(date '+%Y-%m-%dT%H:%M:%S%z') === deadline $deadline_human reached after $n attempts, ALL AUTH-REJECTED — this watch proves NOTHING about publication; fix LS_KRX_APPKEY and probe by hand ==="
      exit 41
    fi
    log_line "$(date '+%Y-%m-%dT%H:%M:%S%z') === deadline $deadline_human reached after $n attempts ($answered answered), no positive — stopping ==="
    exit 40
  fi
  sleep "$interval"
done
