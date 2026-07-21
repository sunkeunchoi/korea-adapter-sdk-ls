#!/usr/bin/env python3
"""Independent twin of the stop-width-geometry four-signal screen (plan 2026-07-21-001).

The GO/STOP verdict is load-bearing (it decides whether the lever code is written), so per the
pre-code-collinearity-gate convention every gated reading — and the winning-signal argmax
(`winning_signal_id`, tolerance 0) — is recomputed here on a DIFFERENT code path from
`diagnostic.py`: dict-record assembly, a covariance-matrix Pearson, spelled-out linear-interp
percentiles, a per-session minute index keyed by timestamp, and an independently-structured
barrier walk. The barrier SEMANTICS (RangeLow decoupled target/breakeven, close-confirm entry,
stop-first pessimism, breakeven ratchet to entry) must match the strategy — the independence is
in reconstruction and statistics, the parts a shared bug could corrupt. The diagnose stage
STOPs on any reading disagreement beyond the frozen per-reading tolerance, and a winner
disagreement surfaces as a tolerance-0 `winning_signal_id` mismatch.
"""
import datetime as dt
import glob
import json
import math
import struct
import sys
from pathlib import Path

import pyarrow.parquet as pq

RUN_ID = "20260717T094841Z-backtest-orb-v32"
REPO = Path(__file__).resolve().parents[5]
RUN_HOME = REPO / "data" / "turn4-fresh" / "runs" / RUN_ID
BARS = REPO / "data" / "turn4-fresh" / "catalog" / "data" / "bars"

S = 1e9
KST = dt.timezone(dt.timedelta(hours=9))
W = 14
ROPEN, REND, FLAT = dt.time(9, 0), dt.time(9, 20), dt.time(15, 0)
BUDGET, NOTIONAL = 299_340.0, 10_000_000.0
PT_R, BE_R = 1.0, 0.41
RR, RLO, RHI, RA = 0.07315764, 0.70269755, 1.44548956, 1.0
A = 0.5
CT = 0.70
SIG_IDS = [1, 2, 3, 4]
SIG_KEYS = {1: "orwidth_atr", 2: "minutes", 3: "gap", 4: "orposition"}

fx = lambda b: struct.unpack("<q", b)[0] / S
stamp = lambda ns: dt.datetime.fromtimestamp(ns / 1e9, tz=dt.timezone.utc).astimezone(KST)


def daily(sym, cache):
    if sym in cache:
        return cache[sym]
    m = {}
    for fn in sorted(glob.glob(str(BARS / f"{sym}-1-DAY-LAST-EXTERNAL" / "*.parquet"))):
        tab = pq.read_table(fn).to_pydict()
        for i in range(len(tab["ts_event"])):
            m[stamp(tab["ts_event"][i]).date()] = {
                "h": fx(tab["high"][i]), "l": fx(tab["low"][i]),
                "c": fx(tab["close"][i]),
            }
    cache[sym] = m
    return m


def minutes_by_session(sym, cache):
    """Per-session sorted list of (ts, high, low, close) — a distinct index from the gate's flat list."""
    if sym in cache:
        return cache[sym]
    sess = {}
    for fn in sorted(glob.glob(str(BARS / f"{sym}-1-MINUTE-LAST-EXTERNAL" / "*.parquet"))):
        tab = pq.read_table(fn, columns=["high", "low", "close", "ts_event"]).to_pydict()
        for i in range(len(tab["ts_event"])):
            ts = tab["ts_event"][i]
            sess.setdefault(stamp(ts).date(), []).append((ts, fx(tab["high"][i]), fx(tab["low"][i]), fx(tab["close"][i])))
    for k in sess:
        sess[k].sort(key=lambda r: r[0])
    cache[sym] = sess
    return sess


def atr14(dmap, session):
    seq = [dmap[d] for d in sorted(dmap) if d < session]
    if len(seq) < W + 1:
        return None
    seq = seq[-(W + 1):]
    total = 0.0
    for k in range(1, len(seq)):
        pc = seq[k - 1]["c"]
        total += max(seq[k]["h"] - seq[k]["l"], abs(seq[k]["h"] - pc), abs(seq[k]["l"] - pc))
    return total / W


def prior_open(dmap, session):
    pri = [d for d in sorted(dmap) if d < session]
    if not pri or session not in dmap:
        return None, None
    return dmap[pri[-1]]["c"], dmap[session]["_open"]


def corr(a, b):
    # covariance-matrix route, distinct from the gate's single-pass Pearson
    n = len(a)
    ma, mb = sum(a) / n, sum(b) / n
    da = [x - ma for x in a]
    db = [y - mb for y in b]
    saa = sum(x * x for x in da)
    sbb = sum(y * y for y in db)
    sab = sum(da[i] * db[i] for i in range(n))
    if saa == 0 or sbb == 0:
        return float("nan")
    return sab / math.sqrt(saa * sbb)


def qtile(vals, q):
    # spelled-out linear interpolation, distinct index math from the gate script
    m = len(vals)
    if m == 1:
        return vals[0]
    idx = q * (m - 1)
    below = int(idx // 1)
    above = min(below + 1, m - 1)
    frac = idx - below
    return vals[below] * (1.0 - frac) + vals[above] * frac


def wclamp(sig, ref, lo, hi):
    # `ref <= 0.0` guarded too — a non-positive base under the fractional exponent is complex
    # and crashes the comparison; fail closed to neutral (defence for reuse on a negative-signal
    # distribution; every v32 signal here is strictly positive).
    if sig is None or sig <= 0.0 or ref <= 0.0:
        return 1.0
    raw = (ref / sig) ** A
    return lo if raw < lo else (hi if raw > hi else raw)


def wratio(atr, px):
    if atr is None or atr <= 0.0 or px <= 0.0:
        return 1.0
    raw = (RR / (atr / px)) ** RA
    return RLO if raw < RLO else (RHI if raw > RHI else raw)


def sizeqty(w, rps_scaled, wr, px):
    a = math.floor(BUDGET * wr / rps_scaled)
    b = math.floor(NOTIONAL / px)
    return a if a < b else b


def walk(sess_bars, entry, rl, rh, entry_ts, stop_dist):
    """Independently structured barrier walk; RangeLow decoupled geometry, same semantics."""
    E = round(entry)
    rdenom = round(rh) - round(rl)
    stop = E - (max(1, round(stop_dist)))
    tgt = E + round(PT_R * rdenom) if rdenom > 0 else None
    betrig = E + round(BE_R * rdenom) if rdenom > 0 else None
    hw = E
    seq = [b for b in sess_bars if b[0] > entry_ts]
    last_close = None
    for ts, hi, lo, cl in seq:
        last_close = cl
        if stamp(ts).time() >= FLAT:
            return "timeflat", round(lo)
        li, hi_i = round(lo), round(hi)
        if li <= stop:
            return "stop", li
        if tgt is not None and hi_i >= tgt:
            return "target", tgt
        if hi_i > hw:
            hw = hi_i
        if betrig is not None and hw >= betrig and stop < E:
            stop = E
    return ("timeflat", round(last_close)) if last_close is not None else ("timeflat", E)


def main():
    out = sys.argv[-1]
    manifest = json.load(open(RUN_HOME / "manifest.json"))
    assert manifest["params"].get("stop_mode", 0.0) == 0.0, "not RangeLow"
    perf = json.load(open(RUN_HOME / "performance.json"))
    dc, mc = {}, {}

    recs = []
    for t in perf["trades"]:
        if t.get("ts_closed") is None:
            continue
        q = t["quantity"]
        rc = t.get("risk_capital")
        if rc is None or q <= 0:
            continue
        sym = t["symbol"]
        session = stamp(t["ts_opened"]).date()
        entry = t["avg_px_open"]
        rps = rc / q
        dmap = daily(sym, dc)
        # today's open needs the daily open — augment the daily map lazily
        if session in dmap and "_open" not in dmap[session]:
            for fn in sorted(glob.glob(str(BARS / f"{sym}-1-DAY-LAST-EXTERNAL" / "*.parquet"))):
                tab = pq.read_table(fn, columns=["open", "ts_event"]).to_pydict()
                for i in range(len(tab["ts_event"])):
                    d = stamp(tab["ts_event"][i]).date()
                    if d in dmap:
                        dmap[d]["_open"] = fx(tab["open"][i])
        atr = atr14(dmap, session)
        pc, topen = prior_open(dmap, session)
        smb = minutes_by_session(sym, mc).get(session, [])
        rh = rl = None
        for ts, hi, lo, _c in smb:
            tt = stamp(ts).time()
            if ROPEN <= tt < REND:
                rh = hi if rh is None else max(rh, hi)
                rl = lo if rl is None else min(rl, lo)
        buy = next(f for f in t["fills"] if f["side"] == "BUY")
        ets = buy["ts_event"]
        et = stamp(ets)
        mins = (et.hour * 60 + et.minute + et.second / 60.0) - 540.0
        rdenom = (rh - rl) if (rh is not None and rl is not None) else None
        recs.append({
            "sym": sym, "session": session, "entry": entry, "rps": rps, "ets": ets,
            "rl": rl, "rh": rh, "bars": smb, "wr": wratio(atr, entry),
            "orwidth_atr": (rdenom / atr) if (rdenom and atr and atr > 0) else None,
            "minutes": mins,
            "gap": ((topen / pc - 1.0) if (pc and pc > 0) else None),
            "orposition": ((entry - rl) / rdenom) if (rdenom and rdenom > 0) else None,
        })

    # baseline sim(w=1)
    bpnl = brc = 0.0
    for r in recs:
        q1 = sizeqty(1.0, max(1.0, r["rps"]), r["wr"], r["entry"])
        res, ex = walk(r["bars"], r["entry"], r["rl"], r["rh"], r["ets"], r["rps"])
        r["res1"] = res
        bpnl += q1 * (ex - round(r["entry"]))
        brc += q1 * max(1.0, r["rps"])
    base = bpnl / brc

    def evaluate(key):
        avail = sorted(r[key] for r in recs if r[key] is not None)
        ref = qtile(avail, 0.5)
        p10 = qtile(avail, 0.10)
        p90 = qtile(avail, 0.90)
        # Non-positive ref/percentiles → complex fractional-power band → crash; fail closed neutral.
        if ref <= 0.0 or p10 <= 0.0 or p90 <= 0.0:
            lo = hi = 1.0
        else:
            lo = (ref / p90) ** A
            hi = (ref / p10) ** A
        coh = [r for r in recs if r[key] is not None]
        ws = [wclamp(r[key], ref, lo, hi) for r in coh]
        c_rps = abs(corr(ws, [r["rps"] for r in coh]))
        c_rat = abs(corr(ws, [r["wr"] for r in coh]))
        pnl = rcc = 0.0
        moved = 0
        for r in recs:
            w = wclamp(r[key], ref, lo, hi) if r[key] is not None else 1.0
            rs = max(1.0, w * r["rps"])
            qw = sizeqty(w, rs, r["wr"], r["entry"])
            res, ex = walk(r["bars"], r["entry"], r["rl"], r["rh"], r["ets"], w * r["rps"])
            pnl += qw * (ex - round(r["entry"]))
            rcc += qw * rs
            if res != r["res1"]:
                moved += 1
        return {"collin_abs_rps": c_rps, "collin_abs_ratio_atr": c_rat,
                "ror_shift": (pnl / rcc) - base, "resolution_mix_shift": moved / len(recs),
                "ref": ref, "lo": lo, "hi": hi}

    ev = {sid: evaluate(SIG_KEYS[sid]) for sid in SIG_IDS}
    clr = [sid for sid in SIG_IDS if (ev[sid]["collin_abs_rps"] < CT and ev[sid]["collin_abs_ratio_atr"] < CT
                                      and ev[sid]["ror_shift"] >= 0.005 and ev[sid]["resolution_mix_shift"] >= 0.05)]
    pool = clr if clr else SIG_IDS
    winner = max(pool, key=lambda sid: (ev[sid]["ror_shift"], -sid))
    w = ev[winner]
    print(f"twin: base RoR {base:.4f} winner id {winner} clearing {clr or 'NONE'}")

    readings = {
        "collin_abs_rps": round(w["collin_abs_rps"], 4),
        "collin_abs_ratio_atr": round(w["collin_abs_ratio_atr"], 4),
        "ror_shift": round(w["ror_shift"], 6),
        "resolution_mix_shift": round(w["resolution_mix_shift"], 4),
        "winning_signal_id": float(winner),
        "stop_width_ref": round(w["ref"], 8),
        "stop_width_w_lo": round(w["lo"], 8),
        "stop_width_w_hi": round(w["hi"], 8),
    }
    with open(out, "w") as fh:
        json.dump(readings, fh, sort_keys=True)


if __name__ == "__main__":
    main()
