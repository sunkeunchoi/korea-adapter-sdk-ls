#!/usr/bin/env python3
"""Pure-relocation verifier for the test-file decomposition (plan 2026-06-30-005).

The STRONGEST silent-regression guard: it catches a test whose body was edited
during a "move" — something the `--list` base-name snapshot (scripts/test-name-
snapshot.sh) cannot see. It asserts that the non-blank line MULTISET of the new
parent + every family file equals the multiset of the ORIGINAL monolith, after
removing the structural additions (`mod <fam>;`, `#[path = "…"]`, `use super::*;`)
and normalizing a moved `include_str!("../fixtures/…")` back to `fixtures/`.

Locale-independent by construction (Python set/Counter, never shell `sort`) — the
equivalent shell `diff <(sort) <(sort)` recipe gives FALSE-POSITIVE diffs on the
Korean fixture strings in these tests unless `LC_ALL=C` is pinned. This script
avoids that trap entirely.

Usage:
  scripts/relocation-check.py <base-ref> <parent-path> <family-dir>
e.g.
  scripts/relocation-check.py main \
      crates/ls-sdk/tests/account_tests.rs crates/ls-sdk/tests/account

Exit 0 + "RELOCATION OK" when the move is pure; exit 1 + the offending lines
otherwise. <base-ref> is any ref where <parent-path> still holds the pre-split
monolith (usually `main` or the merge-base).
"""
import sys, os, glob, subprocess
from collections import Counter


def norm(line):
    return line.replace('include_str!("../fixtures/', 'include_str!("fixtures/')


def is_structural(line):
    s = line.strip()
    if s == "use super::*;":
        return True
    if s.startswith("mod ") and s.endswith(";") and "::" not in s and "{" not in s:
        return True
    if s.startswith('#[path = "') and s.endswith('"]'):
        return True
    return False


def non_blank_multiset(lines):
    c = Counter()
    for line in lines:
        if is_structural(line):
            continue
        if line.strip() == "":
            continue
        c[norm(line)] += 1
    return c


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    base_ref, parent_path, family_dir = sys.argv[1:4]

    original = subprocess.run(
        ["git", "show", f"{base_ref}:{parent_path}"],
        capture_output=True, text=True, check=True,
    ).stdout.splitlines(keepends=True)

    after = list(open(parent_path))
    for fp in sorted(glob.glob(os.path.join(family_dir, "*.rs"))):
        after += open(fp).readlines()

    only_orig = non_blank_multiset(original) - non_blank_multiset(after)
    only_after = non_blank_multiset(after) - non_blank_multiset(original)

    if not only_orig and not only_after:
        print(f"RELOCATION OK: {parent_path} + {family_dir}/ == {base_ref}:{parent_path} "
              "(non-blank line multisets identical)")
        return 0
    print(f"MISMATCH for {parent_path} (+ {family_dir}/) vs {base_ref}")
    for label, c in (("in ORIGINAL only", only_orig), ("in AFTER only", only_after)):
        if c:
            print(f"--- {label} ---")
            for k, v in list(c.items())[:40]:
                print(f"  x{v}: {k!r}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
