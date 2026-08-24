#!/bin/sh
set -eu

repository_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../../.." && pwd -P)"
operator="$repository_root/tools/orca-runner-proof/operator.sh"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

assert_contains() {
    description=$1
    needle=$2
    file=$3
    if ! grep -Fq -- "$needle" "$file"; then
        echo "FAIL: $description" >&2
        echo "expected to find: $needle" >&2
        echo "actual output:" >&2
        sed 's/^/  /' "$file" >&2
        exit 1
    fi
}

assert_action_after_separator() {
    expected_action=$1
    file=$2
    actual_action=$(awk 'seen { print; exit } $0 == "--" { seen = 1 }' "$file")
    if [ "$actual_action" != "$expected_action" ]; then
        fail "expected action $expected_action after --, got $actual_action"
    fi
}

cargo_stub="$fixture/cargo-stub"
cat >"$cargo_stub" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" >"$ORCA_RUNNER_TEST_LOG"
EOF
chmod +x "$cargo_stub"

attempt_id="2026-08-24-attended-01"
state_base="$fixture/state"
argument_log="$fixture/arguments"
stdout_log="$fixture/stdout"
stderr_log="$fixture/stderr"

ORCA_RUNNER_CARGO="$cargo_stub" \
ORCA_RUNNER_ORCA="orca-test" \
ORCA_RUNNER_STATE_BASE="$state_base" \
ORCA_RUNNER_TEST_LOG="$argument_log" \
    "$operator" prepare "$attempt_id" >"$stdout_log" 2>"$stderr_log"

cat >"$fixture/expected-arguments" <<EOF
+1.96.0
run
--locked
--quiet
--manifest-path
$repository_root/tools/orca-runner-proof/Cargo.toml
--
prepare
--repository-root
$repository_root
--state-root
$state_base/$attempt_id
--orca
orca-test
EOF

diff -u "$fixture/expected-arguments" "$argument_log" || fail "operator forwarded unexpected arguments"
assert_contains "operator reports the selected action" "action=prepare" "$stderr_log"
assert_contains "operator reports the durable state root" "state_root=$state_base/$attempt_id" "$stderr_log"

unset ORCA_RUNNER_STATE_BASE
XDG_STATE_HOME="$fixture/xdg-state" \
ORCA_RUNNER_CARGO="$cargo_stub" \
ORCA_RUNNER_ORCA="orca-test" \
ORCA_RUNNER_TEST_LOG="$argument_log" \
    "$operator" status "xdg-attempt" >"$stdout_log" 2>"$stderr_log"
assert_contains \
    "operator derives the XDG state convention" \
    "$fixture/xdg-state/korea-adapter-sdk-ls/orca-runner/xdg-attempt" \
    "$argument_log"

unset ORCA_RUNNER_STATE_BASE XDG_STATE_HOME
HOME="$fixture/home" \
ORCA_RUNNER_CARGO="$cargo_stub" \
ORCA_RUNNER_ORCA="orca-test" \
ORCA_RUNNER_TEST_LOG="$argument_log" \
    "$operator" status "home-attempt" >"$stdout_log" 2>"$stderr_log"
assert_contains \
    "operator derives the HOME state convention" \
    "$fixture/home/.local/state/korea-adapter-sdk-ls/orca-runner/home-attempt" \
    "$argument_log"

for action in prepare resume status cancel retry; do
    : >"$argument_log"
    MAKEFLAGS='' \
    MFLAGS='' \
    ORCA_RUNNER_CARGO="$cargo_stub" \
    ORCA_RUNNER_ORCA="orca-test" \
    ORCA_RUNNER_STATE_BASE="$state_base" \
    ORCA_RUNNER_TEST_LOG="$argument_log" \
        make -s -C "$repository_root" \
        "orca-runner-$action" \
        "ORCA_RUNNER_ATTEMPT=make-mapping" \
        >"$stdout_log" 2>"$stderr_log"
    assert_action_after_separator "$action" "$argument_log"
done

injection_marker="$fixture/injection-marker"
injection_id="bad\"; touch $injection_marker; #"
: >"$argument_log"
if MAKEFLAGS='' \
    MFLAGS='' \
    ORCA_RUNNER_CARGO="$cargo_stub" \
    ORCA_RUNNER_ORCA="orca-test" \
    ORCA_RUNNER_STATE_BASE="$state_base" \
    ORCA_RUNNER_TEST_LOG="$argument_log" \
    make -s -C "$repository_root" \
    orca-runner-status \
    "ORCA_RUNNER_ATTEMPT=$injection_id" \
    >"$stdout_log" 2>"$stderr_log"; then
    fail "Make accepted a metacharacter attempt id"
fi
if [ -e "$injection_marker" ]; then
    fail "Make executed attempt-id shell syntax"
fi
if [ -s "$argument_log" ]; then
    fail "invalid Make attempt id reached cargo"
fi

assert_rejected() {
    description=$1
    action=$2
    rejected_id=$3
    rejected_base=$4
    : >"$argument_log"
    rejected_exit=0
    if ORCA_RUNNER_CARGO="$cargo_stub" \
        ORCA_RUNNER_STATE_BASE="$rejected_base" \
        ORCA_RUNNER_TEST_LOG="$argument_log" \
        "$operator" "$action" "$rejected_id" >"$stdout_log" 2>"$stderr_log"; then
        fail "$description was accepted"
    else
        rejected_exit=$?
    fi
    if [ "$rejected_exit" -ne 64 ]; then
        fail "$description exited $rejected_exit instead of 64"
    fi
    if [ -s "$argument_log" ]; then
        fail "$description reached cargo"
    fi
}

assert_rejected "unknown action" "launch" "safe-attempt" "$state_base"
assert_rejected "empty attempt id" "prepare" "" "$state_base"
assert_rejected "path-like attempt id" "prepare" "../escape" "$state_base"
assert_rejected "period-only attempt id" "prepare" "..." "$state_base"
long_id="$(printf '%129s' '' | tr ' ' x)"
assert_rejected "attempt id longer than 128 characters" "prepare" "$long_id" "$state_base"
assert_rejected "relative state base" "prepare" "safe-attempt" "relative/state"
assert_rejected "filesystem-root state base" "prepare" "safe-attempt" "/"
assert_rejected \
    "repository-local state base" \
    "prepare" \
    "safe-attempt" \
    "$repository_root/.orca-runner-state"

echo "orca-runner-operator-check: PASS"
