# candidates/ — frozen turn inputs (git-tracked)

Each subdirectory is one **candidate**: the frozen input to a single
merit-bearing strategy-loop turn. Unlike run artifacts (which stay in the
gitignored `data/` home), candidates are **git-tracked** so commit history is the
freeze evidence the governed turn relies on — a flip refuses if a candidate's
`candidate.json`, diagnostic, or twin is edited after its gate verdict was
written (R1, R2, KTD2).

## Layout

```
candidates/<slug>/
  candidate.json    # the machine-readable pre-register (schema below)
  diagnostic.py     # the Phase-A diagnostic (declared + content-hashed)
  twin.py           # an INDEPENDENT re-implementation of the same statistic
  gate-verdict.json # WRITTEN by `lab-research turn diagnose` — a command OUTPUT,
                    # NOT a frozen input (excluded from the dirty/freeze checks)
```

`candidate.json` is the **one** machine-readable home for the gate thresholds;
accompanying prose (a human PRE-REGISTER) never carries a second copy for a tool
to read. See `example/` for a fully-worked, offline-loadable template and
`crate::candidates` for the schema.

## Freeze discipline

`lab-research turn diagnose` refuses when any frozen input
(`candidate.json` + the declared diagnostic + twin) is git-dirty, and records the
freeze commit in the gate verdict. `gate-verdict.json` is a command output, so it
never trips the dirty check — a GO written earlier in an invocation chain is
reusable uncommitted. Commit the candidate's frozen inputs (and, per the skill's
standing rule, the turn's ledger record and artifacts) with the turn.

## Diagnostic contract (KTD3)

Each declared script is run as its `argv` with **one appended argument**: the
path to write a canonical `readings.json` to. The file must contain every
declared reading key, rounded to the pre-registered precision. The diagnose stage
compares the diagnostic's and twin's readings **reading-by-reading within the
per-reading tolerance** — raw-stdout byte comparison is not the gate, since two
independently-authored twins never produce byte-identical output. In practice the
scripts ride `uv run --with pyarrow python3 …` (pyarrow is absent from local
pythons); the wrapper is interpreter-agnostic.
