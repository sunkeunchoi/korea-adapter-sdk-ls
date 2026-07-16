#!/usr/bin/env python3
"""Example independent twin (U4/U5 template).

An INDEPENDENT re-implementation of the diagnostic's statistic — authored
separately, not copied — so the diagnose stage can bit-compare the two canonical
readings within the pre-registered per-reading tolerance and catch a coding slip
in either. Raw-stdout byte comparison is deliberately NOT the gate: two
independently-authored twins never produce byte-identical output.

Same contract as the diagnostic: the wrapper appends the readings output path as
the final argument.
"""
import json
import sys

out_path = sys.argv[-1]

# Independent computation lands on the same value within tolerance (here exact,
# for a deterministic example).
readings = {
    "collinearity_r": round(-0.3617, 4),
}

with open(out_path, "w") as fh:
    json.dump(readings, fh, sort_keys=True)
