#!/usr/bin/env bash
# Test-name snapshot for the test-file decomposition (plan 2026-06-30-005, U1/KD3).
#
# Emits the sorted set of BASE test names (the final `::` segment) for one ls-sdk
# integration-test binary. Moving tests into named submodules prefixes their
# `--list` path (`foo` -> `family::foo`); the base-name set + count is the
# silent-drop invariant that must be identical before and after a split (R2).
#
# Usage:
#   scripts/test-name-snapshot.sh <bin> > before.txt    # before the split
#   scripts/test-name-snapshot.sh <bin> > after.txt     # after the split
#   diff before.txt after.txt                           # MUST be empty
#
# Add --ignored to capture only the #[ignore] set (the attribute invariant, R1):
#   scripts/test-name-snapshot.sh <bin> --ignored
set -euo pipefail

bin="${1:?usage: test-name-snapshot.sh <bin> [--ignored]}"
shift || true

cargo test -p ls-sdk --test "$bin" -- --list "$@" 2>/dev/null \
  | grep ': test$' \
  | sed -E 's/^.*:://' \
  | sort
