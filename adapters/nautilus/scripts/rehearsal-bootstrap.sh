#!/usr/bin/env bash
# rehearsal-bootstrap.sh — U11 (plan 2026-09-08-1215, R23, KTD10, KTD11): create the REHEARSAL
# daily home by cloning the frozen JUDGMENT home once.
#
# TWO DAILY HOMES, kept apart on purpose. `data/next-daily-2016` is the lineage judgment catalog:
# its bars back the committed holdout fingerprint, and a `FROZEN-20260812` marker makes every
# catalog write refuse (nautilus_ls::ingest::ensure_catalog_writable, PR #314). The rehearsal
# trades a copy of it, and ONLY the copy advances by the morning t8410 accumulate
# (`LS_SM_PROFILE=daily-rehearsal ./session-morning.sh`). This script makes that copy.
#
# WHAT IS COPIED, and what is not:
#   * `catalog/` — the bars, the ingest checkpoint, and the universe-metadata pin. Byte-for-byte.
#   * NOT the marker. A clone carrying `FROZEN-20260812` could never advance, so the marker is
#     excluded wherever it sits (the canonical home root is outside `catalog/` anyway; a misplaced
#     copy inside it is excluded too), and the clone is re-checked for one before it is published.
#   * NOT lock files (`*.lock`). A copied `.ls-ingest.lock` blocks every later ingest until an
#     operator deletes it, and a copied `.ls-live.lock` would do the same to the rehearsal mount.
#   * NOT `runs/`. Those are the judgment home's backtest runs (U4 candidates, the eventual holdout
#     run). Nothing on the rehearsal path reads them, and a copy would put judgment artifacts under
#     a home whose catalog no longer matches the one they were produced from.
#
# WHAT IS CREATED beside it: `rehearsal/` holding an EMPTY `book.json` — the flat-start book
# `RehearsalBook::empty("")` serializes to, whose empty stamp `assert_fresh` exempts — and
# `state/`, where step [7]'s spend ledger lives.
#
# It never overwrites. An existing destination is a rehearsal home that may hold an overnight book,
# so re-running is a refusal, not a refresh. To start over, move the old home aside by hand.
#
# Usage:
#   ./rehearsal-bootstrap.sh            # clone data/next-daily-2016 -> data/rehearsal-daily
#   ./rehearsal-bootstrap.sh --dry-run  # print what would be done, write nothing
#
# Exit codes:
#   0   the rehearsal home was created and verified
#   1   the copy or its verification failed; nothing was published (the partial copy is removed)
#   64  refused before any write: bad argument, missing or unusable source, destination exists,
#       or a symlink inside the source catalog
set -uo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
R="$(cd -- "$script_dir/../../.." && pwd)"          # the ONE repo-root variable
SOURCE_HOME="$R/data/next-daily-2016"
DEST_HOME="$R/data/rehearsal-daily"
# The marker's name is the Rust constant nautilus_ls::ingest::FROZEN_CATALOG_MARKER;
# session-morning.test.sh asserts the two spellings agree.
FROZEN_MARKER="FROZEN-20260812"
# The empty book. Field order and values are RehearsalBook::empty("") with BOOK_VERSION and
# SESSION_ORDINAL_EPOCH from adapters/nautilus/lab/src/runner/live_daily.rs; the harness checks both
# constants against that file.
BOOK_VERSION=2
SESSION_ORDINAL_EPOCH="2010-01-04"

dry_run=0
for a in "$@"; do case "$a" in
  --dry-run) dry_run=1 ;;
  *) echo "error: unknown argument '$a'" >&2; exit 64 ;;
esac; done

refuse() { printf 'error: %s\n' "$*" >&2; exit 64; }
fail()   { printf '\nFAILED: %s\n' "$*" >&2; exit 1; }
say()    { printf '%s\n' "$*"; }

[[ -d "$SOURCE_HOME/catalog" ]] \
  || refuse "no source catalog at $SOURCE_HOME/catalog — the judgment home is the only thing this clones."
[[ -r "$SOURCE_HOME/catalog/ingest-checkpoint.json" ]] \
  || refuse "the source catalog has no ingest-checkpoint.json; a clone without watermarks cannot accumulate."
if [[ -e "$DEST_HOME" || -L "$DEST_HOME" ]]; then
  refuse "$DEST_HOME already exists. It may hold an overnight rehearsal book, so this script never
       overwrites it. Move it aside by hand if you really mean to start the rehearsal home over."
fi
# A symlink inside the source would be copied as a link, and a later accumulate into the clone would
# write THROUGH it into the frozen judgment catalog. Refuse rather than deciding what it meant.
links="$(find "$SOURCE_HOME/catalog" -type l | head -5)"
[[ -z "$links" ]] || refuse "the source catalog contains symlinks, which a clone would share with the
       judgment home (first: ${links%%$'\n'*}). Resolve them in the source first."

say "source       $SOURCE_HOME/catalog"
say "destination  $DEST_HOME"
say "excluded     *.lock, $FROZEN_MARKER, and everything outside catalog/ (runs/ included)"
say "created      $DEST_HOME/rehearsal/book.json (empty, version $BOOK_VERSION), $DEST_HOME/state/"
if (( dry_run )); then
  say "dry run — nothing written."
  exit 0
fi

# Build under a private sibling name and publish with ONE rename, so an interrupted run never leaves
# something at $DEST_HOME that looks like a usable home. The trap removes only that private path.
staging="$DEST_HOME.partial-$$"
trap 'rm -rf "$staging"' EXIT
mkdir -p "$staging" || fail "could not create $staging"

python3 - "$SOURCE_HOME/catalog" "$staging/catalog" "$FROZEN_MARKER" <<'PY' || fail "copying the catalog failed"
import shutil, sys
src, dst, marker = sys.argv[1:4]
shutil.copytree(src, dst, ignore=shutil.ignore_patterns("*.lock", marker))
PY

mkdir -p "$staging/rehearsal" "$staging/state" || fail "could not create rehearsal/ and state/"
python3 - "$staging/rehearsal/book.json" "$BOOK_VERSION" "$SESSION_ORDINAL_EPOCH" <<'PY' \
  || fail "writing the empty book failed"
import json, sys
path, version, epoch = sys.argv[1], int(sys.argv[2]), sys.argv[3]
with open(path, "w") as handle:
    json.dump({"version": version, "session_date": "", "run_id": "",
               "ordinal_epoch": epoch, "legs": []}, handle, indent=2)
    handle.write("\n")
PY

# Verify the clone before publishing it, not after.
cmp -s "$SOURCE_HOME/catalog/ingest-checkpoint.json" "$staging/catalog/ingest-checkpoint.json" \
  || fail "the cloned ingest checkpoint differs from the source"
leftover="$(find "$staging" \( -name "$FROZEN_MARKER" -o -name '*.lock' \) | head -1)"
[[ -z "$leftover" ]] || fail "the clone still carries $leftover"
src_count="$(find "$SOURCE_HOME/catalog" -type f ! -name '*.lock' ! -name "$FROZEN_MARKER" | wc -l | tr -d ' ')"
dst_count="$(find "$staging/catalog" -type f | wc -l | tr -d ' ')"
[[ "$src_count" == "$dst_count" ]] \
  || fail "the clone holds $dst_count catalog file(s) but the source has $src_count to copy"

# Re-checked here, because `mv` onto a directory that appeared meanwhile would nest the clone
# INSIDE it rather than refusing.
[[ -e "$DEST_HOME" || -L "$DEST_HOME" ]] && fail "$DEST_HOME appeared while the clone was being built"
mv "$staging" "$DEST_HOME" || fail "could not publish $staging as $DEST_HOME"
trap - EXIT
say "rehearsal home ready: $DEST_HOME ($dst_count catalog files)"
say "next: LS_SM_PROFILE=daily-rehearsal ./session-morning.sh --dry-run"
exit 0
