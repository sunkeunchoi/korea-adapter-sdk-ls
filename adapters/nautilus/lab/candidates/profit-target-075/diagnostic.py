#!/usr/bin/env python3
"""Phase-A direction+materiality gate for profit_target_r 1.00 -> 0.75 on v34 (plan 2026-07-24-001).

An EXIT-GEOMETRY lever, not a sizing one: lowering the target reallocates exit *timing*, not
the per-trade risk budget, so the sizing dual-gate (collinearity vs `risk_per_share` +
magnitude) is meaningless here. The gate is instead a **direction + materiality** pair on an
MFE counterfactual over the real-data head cohort.

Counterfactual (KTD2, conservative by construction): for each closed trade,

    r_new = 0.75  if mfe_r >= 0.75   (the trade touched 0.75R -> the lower target fills at or
                                      before its actual exit, booking ~+0.75R)
          = realized_r  otherwise    (never reached 0.75R -> untouched; breakeven arms at 0.41R
                                      independent of the target, the range-low stop is unchanged)

The engine's marketable-limit fill books **at or above** 0.75R on a gap-through, so the actual
flip RoR is >= this counterfactual — the gate can only under-state the flip's edge, never
over-state it (contrast amihud, whose notional ceiling gave a reversing over-prediction). A STOP
is therefore a trustworthy signal; a GO is necessary, not sufficient (the v35 backtest decides).

Readings (mirrored in candidate.json, the machine home):
  ror_base         = size-invariant RoR at the head target 1.00 = sum(rc*r)/sum(rc)
  ror_prime        = size-invariant RoR at the 0.75 counterfactual = sum(rc*r_new)/sum(rc)
  ror_delta        = ror_prime - ror_base   (SIGNED — the load-bearing direction reading, KTD3)
  exit_change_frac = fraction of trades whose booked outcome changes under 0.75

Frozen dual gate (R4):
  ror_delta        >= 0.00065   (direction + magnitude — the honest STOP gate)
  exit_change_frac >= 0.05      (at least 5% of trades change outcome)

R = range_high - range_low, and the head runs the range-low stop (`stop_mode = 0.0`), so `mfe_r`
and the 0.75R target share a denominator and compare directly (KTD4). This candidate is a
v34/range-low-era snapshot — a future head that moved the stop mode invalidates the cohort.

The join mirrors `report_mfe` (report.rs:498-584): exit envelopes' `mfe_r` keyed on
(symbol, KST session date) joined to `performance.json` closed trades keyed on
(symbol, KST session date of ts_opened). No minute-bar replay, so this is plain JSON.
"""
import datetime
import json
import os
import sys

DATA = "/Users/mini/dev/korea-adapter-sdk-ls/data/turn4-fresh"
# Frozen anchor: the real-data head-params twin, catalog fingerprint 363f199d (KTD5).
# LS_PT075_RUN overrides the run DIR for the fixture harness only; diagnose never sets it,
# so the frozen v34 path is what the gate reads.
RUN = os.environ.get("LS_PT075_RUN", f"{DATA}/runs/20260724T014752Z-backtest-orb-v34")

TARGET = 0.75
ROR_DELTA_FLOOR = 0.00065
EXIT_CHANGE_FLOOR = 0.05
EXIT_KINDS = ("stop_hit", "target", "time_exit")
KST_OFFSET = datetime.timedelta(hours=9)


def kst_date_from_ns(ns):
    """KST session date of a unix-nanosecond timestamp (matches report.rs kst_date_of)."""
    dt = datetime.datetime(1970, 1, 1) + datetime.timedelta(microseconds=ns / 1000)
    return (dt + KST_OFFSET).date()


def load_trades(perf_path):
    """Closed trades with risk_capital present and quantity>0 -> per-trade join rows."""
    perf = json.load(open(perf_path))
    rows = []  # (symbol, session_date, risk_capital, realized_r)
    for t in perf["trades"]:
        if t.get("ts_closed") is None:
            continue
        rc = t.get("risk_capital")
        if rc is None or t["quantity"] <= 0:
            continue
        sess = kst_date_from_ns(t["ts_opened"])
        rows.append((t["symbol"], sess, rc, t["realized_r"]))
    return rows


def load_exit_mfe(decisions_path):
    """Exit envelopes' mfe_r keyed on (symbol, KST date of ts_event) — the report.rs partition."""
    mfe = {}
    for line in open(decisions_path):
        line = line.strip()
        if not line:
            continue
        env = json.loads(line)
        d = env.get("decision_detail")
        if not d or d.get("kind") not in EXIT_KINDS:
            continue
        vals = d.get("values", {})
        if "mfe_r" not in vals:  # predates turn-8 telemetry — never read as 0
            continue
        mfe[(d["symbol"], kst_date_from_ns(env["ts_event"]))] = vals["mfe_r"]
    return mfe


def main():
    out_path = sys.argv[-1]
    trades = load_trades(f"{RUN}/performance.json")
    mfe = load_exit_mfe(f"{RUN}/decisions.jsonl")

    # Inner-join: drop (and count) trades with no mfe_r record, like report_mfe's exits_without_mfe.
    joined = []  # (risk_capital, realized_r, mfe_r)
    exits_without_mfe = 0
    for (sym, sess, rc, r) in trades:
        m = mfe.get((sym, sess))
        if m is None:
            exits_without_mfe += 1
            continue
        joined.append((rc, r, m))

    n = len(joined)
    if n == 0:
        sys.stderr.write(
            f"FATAL: zero join rows over {RUN} "
            f"({len(trades)} closed trades, {len(mfe)} exit mfe records) — "
            "the anchor run has no mfe_r-bearing cohort (stale anchor or wrong run).\n"
        )
        sys.exit(2)

    num = sum(rc * r for (rc, r, _) in joined)
    den = sum(rc for (rc, _, _) in joined)
    ror_base = num / den

    def r_new(rc, r, m):
        return TARGET if m >= TARGET else r

    num_p = sum(rc * r_new(rc, r, m) for (rc, r, m) in joined)
    ror_prime = num_p / den
    ror_delta = ror_prime - ror_base

    n_change = sum(
        1 for (rc, r, m) in joined if m >= TARGET and abs(r_new(rc, r, m) - r) > 1e-9
    )
    exit_change_frac = n_change / n

    cond_dir = ror_delta >= ROR_DELTA_FLOOR
    cond_mat = exit_change_frac >= EXIT_CHANGE_FLOOR
    dual_go = cond_dir and cond_mat

    # --- human-readable report (stdout; the gate reads only the JSON file) ---
    print(f"run: {RUN}")
    print(f"closed trades:              {len(trades)}")
    print(f"exit mfe records:           {len(mfe)}")
    print(f"joined cohort (n):          {n}   (excluded {exits_without_mfe} without mfe_r)")
    print()
    print(f"RoR_base  (target 1.00):    {ror_base:.6f}")
    print(f"RoR_prime (target 0.75):    {ror_prime:.6f}")
    print(f"ror_delta (signed):         {ror_delta:.6f}   >= {ROR_DELTA_FLOOR} -> "
          f"{'PASS' if cond_dir else 'STOP'}")
    print(f"exit_change_frac:           {exit_change_frac:.4f} ({n_change}/{n})   >= "
          f"{EXIT_CHANGE_FLOOR} -> {'PASS' if cond_mat else 'STOP'}")
    print()
    print(f"=== PHASE-A DECISION: {'DUAL GO' if dual_go else 'STOP'} "
          f"(direction {'PASS' if cond_dir else 'STOP'}, "
          f"materiality {'PASS' if cond_mat else 'STOP'}) ===")

    # --- canonical readings artifact (the gate reads THIS) ---
    readings = {
        "ror_base": round(ror_base, 6),
        "ror_prime": round(ror_prime, 6),
        "ror_delta": round(ror_delta, 6),
        "exit_change_frac": round(exit_change_frac, 4),
    }
    with open(out_path, "w") as fh:
        json.dump(readings, fh, sort_keys=True)


if __name__ == "__main__":
    main()
