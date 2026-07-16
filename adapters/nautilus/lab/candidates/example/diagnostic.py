#!/usr/bin/env python3
"""Example Phase-A diagnostic (U4/U5 template).

A real diagnostic computes its readings from the run's closed-trade parquet
(via `uv run --with pyarrow python3 diagnostic.py <out>` — pyarrow is absent
from local pythons, so it rides `uv`). This example emits fixed readings so the
candidate loads and round-trips offline with no data dependency.

Contract (KTD3): the wrapper appends ONE argument — the path to write the
canonical `readings.json` to — after the declared argv. Every declared reading
key must appear, rounded to the pre-registered precision.
"""
import json
import sys

# The wrapper appends the output path as the final argv entry.
out_path = sys.argv[-1]

readings = {
    # Collinearity of the new normalizer against the existing risk-per-share —
    # the pre-code gate that must clear before a second normalizer is built.
    "collinearity_r": round(-0.3617, 4),
}

with open(out_path, "w") as fh:
    json.dump(readings, fh, sort_keys=True)
