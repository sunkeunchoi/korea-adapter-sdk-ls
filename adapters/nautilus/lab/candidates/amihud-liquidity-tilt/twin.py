#!/usr/bin/env python3
"""Independent twin of the Amihud-illiquidity budget-tilt Phase-A gate (plan 2026-07-16-003).

The GO/STOP verdict is load-bearing (it decides whether the lever code is written), so per the
pre-code-collinearity-gate convention every gated reading is recomputed here via a DIFFERENT
code path (no functions shared with diagnostic.py): record-dict assembly, inline linear-interp
percentiles with spelled-out index math, an independent Pearson, and an independently-structured
Amihud aggregation. It emits the same four canonical readings; the diagnose stage STOPs on any
disagreement beyond the frozen per-reading tolerance.
"""
import json, struct, glob, math, datetime, sys

DATA = "/Users/mini/dev/korea-adapter-sdk-ls/data/turn4-fresh"
V30 = f"{DATA}/runs/20260715T092847Z-backtest-orb-v30/performance.json"
CATALOG = f"{DATA}/catalog/data/bars"
W = 14
S = 1e9
KST = datetime.timedelta(hours=9)
RPT = 299_340.0
NOTIONAL = 10_000_000.0
R_REF = 0.07315764
R_WLO = 0.70269755
R_WHI = 1.44548956
ALPHA = 1.0
THRESH = 0.70
SHIFT_FLOOR = 0.00065
QTY_FLOOR = 0.05

import pyarrow.parquet as pq

fx = lambda b: struct.unpack("<q", b)[0] / S
kd = lambda ns: (datetime.datetime(1970, 1, 1) + datetime.timedelta(microseconds=ns / 1000) + KST).date()


def daily(sym):
    rows = {}
    for f in sorted(glob.glob(f"{CATALOG}/{sym}-1-DAY-LAST-EXTERNAL/*.parquet")):
        d = pq.read_table(f).to_pydict()
        for i in range(len(d["ts_event"])):
            rows[kd(d["ts_event"][i])] = {
                "h": fx(d["high"][i]), "l": fx(d["low"][i]),
                "c": fx(d["close"][i]), "v": fx(d["volume"][i]),
            }
    return rows


def priors_before(rows, sess):
    return [rows[dt] for dt in sorted(rows) if dt < sess]


def amihud(rows, sess):
    seq = priors_before(rows, sess)
    if len(seq) < W + 1:
        return None
    seq = seq[-(W + 1):]
    acc = 0.0
    cnt = 0
    for k in range(1, len(seq)):
        pc = seq[k - 1]["c"]
        c = seq[k]["c"]
        turn = c * seq[k]["v"]
        if pc <= 0.0 or turn <= 0.0:
            return None
        acc += abs(c / pc - 1.0) / turn
        cnt += 1
    return acc / cnt if cnt else None


def atr(rows, sess):
    seq = priors_before(rows, sess)
    if len(seq) < W + 1:
        return None
    seq = seq[-(W + 1):]
    tot = 0.0
    for k in range(1, len(seq)):
        pc = seq[k - 1]["c"]
        tot += max(seq[k]["h"] - seq[k]["l"], abs(seq[k]["h"] - pc), abs(seq[k]["l"] - pc))
    return tot / W


def corr(a, b):
    n = len(a)
    ma = sum(a) / n
    mb = sum(b) / n
    sab = sum((a[i] - ma) * (b[i] - mb) for i in range(n))
    saa = math.sqrt(sum((x - ma) ** 2 for x in a))
    sbb = math.sqrt(sum((x - mb) ** 2 for x in b))
    return sab / (saa * sbb)


def q_pct(vals, q):
    # spelled-out linear interpolation, distinct structure from the gate script
    m = len(vals)
    idx = q * (m - 1)
    below = int(idx // 1)
    above = min(below + 1, m - 1)
    frac = idx - below
    return vals[below] * (1.0 - frac) + vals[above] * frac


def w_clamp(x, ref, lo, hi):
    if x is None or x <= 0.0:
        return 1.0
    raw = (ref / x) ** ALPHA
    if raw < lo:
        return lo
    if raw > hi:
        return hi
    return raw


perf = json.load(open(V30))
recs = []
cache = {}
for t in perf["trades"]:
    if t.get("ts_closed") is None:
        continue
    q = t["quantity"]
    rc = t.get("risk_capital")
    if rc is None or q <= 0:
        continue
    sym = t["symbol"]
    if sym not in cache:
        cache[sym] = daily(sym)
    sess = kd(t["ts_opened"])
    il = amihud(cache[sym], sess)
    a = atr(cache[sym], sess)
    px = t["avg_px_open"]
    v = (a / px) if (a is not None and a > 0 and px > 0) else None
    recs.append({
        "rps": rc / q, "rc": rc, "r": t["realized_r"], "px": px,
        "illiq": il if (il is not None and il > 0.0) else None,
        "w_ratio": w_clamp(v, R_REF, R_WLO, R_WHI),
    })

avail = sorted(x["illiq"] for x in recs if x["illiq"] is not None)
ref = q_pct(avail, 0.5)
p10 = q_pct(avail, 0.10)
p90 = q_pct(avail, 0.90)
wlo = ref / p90
whi = ref / p10

for x in recs:
    x["w"] = w_clamp(x["illiq"], ref, wlo, whi)

# Gate 1a / 1b over illiq cohort
coh = [x for x in recs if x["illiq"] is not None]
r_rps = corr([x["w"] for x in coh], [x["rps"] for x in coh])
r_ratio = corr([x["w"] for x in coh], [x["w_ratio"] for x in coh])

# Gate 2 over all rows
ror = sum(x["rc"] * x["r"] for x in recs) / sum(x["rc"] for x in recs)
ror_p = sum(x["w"] * x["rc"] * x["r"] for x in recs) / sum(x["w"] * x["rc"] for x in recs)
shift = abs(ror_p - ror)
nch = 0
for x in recs:
    qn = min(math.floor(RPT * x["w"] / x["rps"]), math.floor(NOTIONAL / x["px"]))
    qo = min(math.floor(RPT / x["rps"]), math.floor(NOTIONAL / x["px"]))
    if qn != qo:
        nch += 1
frac = nch / len(recs)

print(f"twin: illiq-cohort n={len(coh)} of {len(recs)}   ref={ref:.6e} wlo={wlo:.6f} whi={whi:.6f}")
print(f"twin: |r(w,rps)|={abs(r_rps):.4f}  |r(w,w_ratio)|={abs(r_ratio):.4f}  shift={shift:.6f}  qty_frac={frac:.4f}")
print(f"twin verdict: 1a {'GO' if abs(r_rps)<THRESH else 'STOP'} 1b {'GO' if abs(r_ratio)<THRESH else 'STOP'} "
      f"2 {'GO' if (shift>=SHIFT_FLOOR and frac>=QTY_FLOOR) else 'STOP'}")

readings = {
    "collin_abs_rps": round(abs(r_rps), 4),
    "collin_abs_ratio_atr": round(abs(r_ratio), 4),
    "ror_shift": round(shift, 6),
    "qty_change_frac": round(frac, 4),
}
with open(sys.argv[-1], "w") as fh:
    json.dump(readings, fh, sort_keys=True)
