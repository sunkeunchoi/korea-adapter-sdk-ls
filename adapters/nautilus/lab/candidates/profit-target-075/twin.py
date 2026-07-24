#!/usr/bin/env python3
"""Independent twin of the profit_target_r 1.00 -> 0.75 Phase-A gate (plan 2026-07-24-001).

The GO/STOP verdict is load-bearing (it decides whether the flip runs), so per the governed
convention every gated reading is recomputed here via a DIFFERENT code path from diagnostic.py
(no shared functions): a dict-of-lists join representation built exit-first, a single fold that
accumulates the base and counterfactual pnl products together, and a separately-structured
change tally. It emits the same four canonical readings; diagnose STOPs on any disagreement
beyond the frozen per-reading tolerance.
"""
import datetime
import json
import os
import sys

DATA = "/Users/mini/dev/korea-adapter-sdk-ls/data/turn4-fresh"
RUN = os.environ.get("LS_PT075_RUN", f"{DATA}/runs/20260724T014752Z-backtest-orb-v34")

TGT = 0.75
DIR_FLOOR = 0.00065
MAT_FLOOR = 0.05
KST = datetime.timedelta(hours=9)

kd = lambda ns: (datetime.datetime(1970, 1, 1) + datetime.timedelta(microseconds=ns / 1000) + KST).date()


def exit_index(path):
    # exit-first: map (symbol, KST date) -> mfe_r, built by scanning decisions once.
    idx = {}
    for raw in open(path):
        s = raw.strip()
        if not s:
            continue
        obj = json.loads(s)
        det = obj.get("decision_detail") or {}
        if det.get("kind") not in {"target", "stop_hit", "time_exit"}:
            continue
        v = det.get("values") or {}
        if "mfe_r" in v:
            idx[(det["symbol"], kd(obj["ts_event"]))] = v["mfe_r"]
    return idx


def trade_records(path):
    # dict-of-lists: parallel arrays over the eligible closed trades.
    perf = json.load(open(path))
    syms, dates, rcs, rs = [], [], [], []
    for t in perf["trades"]:
        if t.get("ts_closed") is None or t.get("risk_capital") is None or t["quantity"] <= 0:
            continue
        syms.append(t["symbol"])
        dates.append(kd(t["ts_opened"]))
        rcs.append(t["risk_capital"])
        rs.append(t["realized_r"])
    return syms, dates, rcs, rs


def main():
    out = sys.argv[-1]
    idx = exit_index(f"{RUN}/decisions.jsonl")
    syms, dates, rcs, rs = trade_records(f"{RUN}/performance.json")

    sum_rc = 0.0
    sum_pnl = 0.0        # sum(rc * realized_r)
    sum_pnl_p = 0.0      # sum(rc * r_new)
    changed = 0
    n = 0
    for i in range(len(syms)):
        m = idx.get((syms[i], dates[i]))
        if m is None:
            continue
        rc = rcs[i]
        r = rs[i]
        touched = m >= TGT
        rn = TGT if touched else r
        sum_rc += rc
        sum_pnl += rc * r
        sum_pnl_p += rc * rn
        if touched and abs(rn - r) > 1e-9:
            changed += 1
        n += 1

    if n == 0:
        sys.stderr.write(f"FATAL(twin): zero join rows over {RUN}\n")
        sys.exit(2)

    ror_base = sum_pnl / sum_rc
    ror_prime = sum_pnl_p / sum_rc
    ror_delta = ror_prime - ror_base
    frac = changed / n

    print(f"twin: n={n}  RoR_base={ror_base:.6f}  RoR_prime={ror_prime:.6f}  "
          f"ror_delta={ror_delta:.6f}  exit_change_frac={frac:.4f} ({changed}/{n})")
    print(f"twin verdict: direction {'PASS' if ror_delta >= DIR_FLOOR else 'STOP'}  "
          f"materiality {'PASS' if frac >= MAT_FLOOR else 'STOP'}")

    readings = {
        "ror_base": round(ror_base, 6),
        "ror_prime": round(ror_prime, 6),
        "ror_delta": round(ror_delta, 6),
        "exit_change_frac": round(frac, 4),
    }
    with open(out, "w") as fh:
        json.dump(readings, fh, sort_keys=True)


if __name__ == "__main__":
    main()
