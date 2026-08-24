#!/bin/sh
set -eu

usage() {
    cat >&2 <<'EOF'
usage: operator.sh <prepare|resume|status|cancel|retry> [attempt-id]

The attempt id must contain only letters, numbers, dots, underscores, or hyphens.
Make targets supply it through ORCA_RUNNER_ATTEMPT; direct calls may pass it second.
State is stored below ORCA_RUNNER_STATE_BASE, XDG_STATE_HOME, or HOME/.local/state.
EOF
    exit 64
}

case "$#" in
    1)
        action=$1
        attempt_id=${ORCA_RUNNER_ATTEMPT:-}
        ;;
    2)
        action=$1
        attempt_id=$2
        ;;
    *) usage ;;
esac

case "$action" in
    prepare | resume | status | cancel | retry) ;;
    *)
        echo "operator.sh: unsupported action: $action" >&2
        usage
        ;;
esac

case "$attempt_id" in
    "" | *[!A-Za-z0-9._-]*)
        echo "operator.sh: invalid attempt id: $attempt_id" >&2
        usage
        ;;
esac
case "$attempt_id" in
    *[A-Za-z0-9_-]*) ;;
    *)
        echo "operator.sh: attempt id must not contain only periods" >&2
        usage
        ;;
esac
if [ "${#attempt_id}" -gt 128 ]; then
    echo "operator.sh: attempt id exceeds 128 characters" >&2
    usage
fi

repository_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd -P)"
if [ -n "${ORCA_RUNNER_STATE_BASE:-}" ]; then
    state_base=$ORCA_RUNNER_STATE_BASE
elif [ -n "${XDG_STATE_HOME:-}" ]; then
    state_base=$XDG_STATE_HOME/korea-adapter-sdk-ls/orca-runner
elif [ -n "${HOME:-}" ]; then
    state_base=$HOME/.local/state/korea-adapter-sdk-ls/orca-runner
else
    echo "operator.sh: set ORCA_RUNNER_STATE_BASE, XDG_STATE_HOME, or HOME" >&2
    exit 64
fi

case "$state_base" in
    /*) ;;
    *)
        echo "operator.sh: state base must be an absolute path: $state_base" >&2
        exit 64
        ;;
esac
if [ "$state_base" = "/" ]; then
    echo "operator.sh: refusing filesystem root as the state base" >&2
    exit 64
fi
state_base=${state_base%/}
state_root=$state_base/$attempt_id

case "$state_root" in
    "$repository_root" | "$repository_root"/*)
        echo "operator.sh: state root must remain outside the repository: $state_root" >&2
        exit 64
        ;;
esac

cargo_command=${ORCA_RUNNER_CARGO:-cargo}
orca_command=${ORCA_RUNNER_ORCA:-orca}

echo "ORCA-RUNNER action=$action attempt=$attempt_id state_root=$state_root" >&2
exec "$cargo_command" +1.96.0 run --locked --quiet \
    --manifest-path "$repository_root/tools/orca-runner-proof/Cargo.toml" -- \
    "$action" \
    --repository-root "$repository_root" \
    --state-root "$state_root" \
    --orca "$orca_command"
