#!/usr/bin/env python3
"""Independent twin of the failed-break-reversal dual-grammar screen (plan 2026-07-22-001).

The GO/STOP verdict is load-bearing (it decides whether the reversal-stream code is written), so per
the pre-code convention every gated reading — and the winning-grammar argmax (`winning_grammar_id`,
tolerance 0) and the winning stop anchor (`stop_anchor_id`, tolerance 0) — is recomputed here on a
DIFFERENT code path from `diagnostic.py`.

Independence axis: the diagnostic reconstructs ENTRY-LOCALLY (each symbol-session loads its own bars
on demand); this twin builds CATALOG-WIDE maps up front — one pass loading every symbol's daily and
minute bars into global dicts keyed by (symbol, session) — then indexes into them. The
breakdown-recovery scan, the barrier walk, and the population scoring are independently structured
(index-driven loops, a resolution-tally dict, an explicit anchor table). The barrier SEMANTICS
(RangeLow decoupled target/breakeven, close-confirm entry so no same-bar stop, stop-first pessimism,
breakeven ratchet to entry) must MATCH the strategy — the independence is in reconstruction and
statistics, the parts a shared bug could corrupt. The diagnose stage STOPs on any reading
disagreement beyond the frozen per-reading tolerance; a winner/anchor disagreement surfaces as a
tolerance-0 mismatch.
"""
import datetime as dt
import glob
import hashlib
import json
import math
import struct
import sys
from pathlib import Path

import pyarrow.parquet as pq

RUN_ID = "20260717T094841Z-backtest-orb-v32"
STRATEGY_VERSION = 32
STRATEGY_HASH = "d7a9820b7356547ac8de0d0b8b11748dea6e83be7168744ef6591a88ce31145e"
CATALOG_FINGERPRINT = "3b6be31bdf8d29a8d774d42d490020d65753455acfc1b2214a0b13f14b589200"
UNIVERSE_HASH = "1e7394ec17d880de86075178305569fb9769ff3b1c025c904e17f53af60035e1"
DATA_START, DATA_END = "20260526", "20260703"

REPO = Path(__file__).resolve().parents[5]
RUN_HOME = REPO / "data" / "turn4-fresh" / "runs" / RUN_ID
BARS = REPO / "data" / "turn4-fresh" / "catalog" / "data" / "bars"

S = 1_000_000_000
KST = dt.timezone(dt.timedelta(hours=9))
W = 14
ROPEN, REND, FLAT = dt.time(9, 0), dt.time(9, 20), dt.time(15, 0)
BUDGET, NOTIONAL = 299_340.0, 10_000_000.0
PT_R, BE_R = 1.0, 0.41
GAP_MIN, ORW_MAX, RETAIN_MIN = 0.6, 0.666, 0.5
RR, RLO, RHI, RA = 0.07315764, 0.70269755, 1.44548956, 1.0
ROR_FLOOR, COUNT_FLOOR = 0.005, 12
G_A, G_B = 1.0, 2.0
ANCHOR_BREAKDOWN, ANCHOR_RANGE = 1.0, 2.0
WIN_LO, WIN_HI = dt.date(2026, 5, 26), dt.date(2026, 7, 3)

price = lambda raw: _intprice(struct.unpack("<q", raw)[0])
stamp = lambda ns: dt.datetime.fromtimestamp(ns / S, tz=dt.timezone.utc).astimezone(KST)


def _intprice(v):
    if v % S != 0:
        raise AssertionError("non-integral canonical KRW/tick price")
    return v // S


def fixed_str(raw, prec):
    v = struct.unpack("<q", raw)[0]
    sign = "-" if v < 0 else ""
    v = abs(v)
    whole, frac = divmod(v, S)
    if prec == 0:
        return f"{sign}{whole}"
    frac //= 10 ** (9 - prec)
    return f"{sign}{whole}.{frac:0{prec}d}"


def catalog_fingerprint():
    lo = int(dt.datetime(2026, 5, 26, tzinfo=KST).timestamp()) * S
    hi = int(dt.datetime(2026, 7, 4, tzinfo=KST).timestamp()) * S - 1
    seen = set()
    for fn in sorted(BARS.glob("*/*.parquet")):
        tab = pq.read_table(fn, columns=["open", "high", "low", "close", "volume", "ts_event"])
        meta = tab.schema.metadata or {}
        bt = meta.get(b"bar_type", b"").decode()
        pp = int(meta.get(b"price_precision", b"-1"))
        sp = int(meta.get(b"size_precision", b"-1"))
        if not (bt and 0 <= pp <= 9 and 0 <= sp <= 9):
            raise AssertionError(f"invalid Parquet metadata: {fn}")
        d = tab.to_pydict()
        for i, ev in enumerate(d["ts_event"]):
            if lo <= ev <= hi:
                seen.add((bt, ev, d["open"][i], d["high"][i], d["low"][i], d["close"][i], d["volume"][i], pp, sp))
    if not seen:
        raise AssertionError("catalog has no bars in the frozen range")
    rows = []
    for bt, ev, o, h, l, c, v, pp, sp in seen:
        rows.append("|".join([bt, str(ev), fixed_str(o, pp), fixed_str(h, pp), fixed_str(l, pp), fixed_str(c, pp), fixed_str(v, sp)]))
    dig = hashlib.sha256()
    for r in sorted(rows):
        dig.update(r.encode()); dig.update(b"\n")
    return dig.hexdigest()


# ---- catalog-wide maps (built once up front) ----

def load_catalog():
    """Return (daily, minute): daily[sym][date]=(o,h,l,c); minute[sym][date]=sorted[(ts,h,l,c)]."""
    daily, minute = {}, {}
    for fn in sorted(BARS.glob("*-1-DAY-LAST-EXTERNAL/*.parquet")):
        sym = fn.parent.name.split("-1-DAY-")[0]
        d = pq.read_table(fn, columns=["open", "high", "low", "close", "ts_event"]).to_pydict()
        book = daily.setdefault(sym, {})
        latest = {}
        for i, ts in enumerate(d["ts_event"]):
            day = stamp(ts).date()
            if day not in latest or ts > latest[day]:
                latest[day] = ts
                book[day] = (price(d["open"][i]), price(d["high"][i]), price(d["low"][i]), price(d["close"][i]))
    # A divergent duplicate minute bar (two rows at one ts disagreeing on OHLC) aborts (the
    # gap-retention precedent's guard); an exact duplicate is de-duplicated. Keyed catalog-wide by
    # (sym, ts) — the independent axis from the diagnostic's per-symbol `seen` dict.
    seen = {}
    for fn in sorted(BARS.glob("*-1-MINUTE-LAST-EXTERNAL/*.parquet")):
        sym = fn.parent.name.split("-1-MINUTE-")[0]
        d = pq.read_table(fn, columns=["high", "low", "close", "ts_event"]).to_pydict()
        book = minute.setdefault(sym, {})
        for i, ts in enumerate(d["ts_event"]):
            bar = (price(d["high"][i]), price(d["low"][i]), price(d["close"][i]))
            key = (sym, ts)
            if key in seen:
                if seen[key] != bar:
                    raise AssertionError(f"divergent duplicate minute bar for {sym} at ts {ts}")
                continue
            seen[key] = bar
            book.setdefault(stamp(ts).date(), []).append((ts, *bar))
    for sym in minute:
        for day in minute[sym]:
            minute[sym][day].sort(key=lambda r: r[0])
    return daily, minute


def atr(book, session):
    seq = [book[d] for d in sorted(book) if d < session]
    if len(seq) < W + 1:
        return None
    seq = seq[-(W + 1):]
    acc = 0.0
    for k in range(1, len(seq)):
        pc = seq[k - 1][3]
        acc += max(seq[k][1] - seq[k][2], abs(seq[k][1] - pc), abs(seq[k][2] - pc))
    return acc / W


def range_bounds(day_bars):
    highs = [h for ts, h, l, c in day_bars if ROPEN <= stamp(ts).time() < REND]
    lows = [l for ts, h, l, c in day_bars if ROPEN <= stamp(ts).time() < REND]
    if not highs or not lows:
        return None, None
    return max(highs), min(lows)


def wratio(a, px):
    if a is None or a <= 0.0 or px <= 0.0:
        return 1.0
    raw = (RR / (a / px)) ** RA
    return RLO if raw < RLO else (RHI if raw > RHI else raw)


def qty(wr, rps, px):
    if rps <= 0.0 or px <= 0.0:
        return 0
    lot_a = math.floor(BUDGET * wr / rps)
    lot_b = math.floor(NOTIONAL / px)
    return lot_a if lot_a < lot_b else lot_b


def walk(day_bars, entry, rh, rl, entry_ts, stop):
    """Independently structured barrier walk; RangeLow decoupled geometry, same semantics."""
    rd = rh - rl
    tgt = entry + round(PT_R * rd) if rd > 0 else None
    betr = entry + round(BE_R * rd) if rd > 0 else None
    hw = entry
    tail = [b for b in day_bars if b[0] > entry_ts]
    lc = None
    for ts, hi, lo, cl in tail:
        lc = cl
        if stamp(ts).time() >= FLAT:
            return "timeflat", lo
        if lo <= stop:
            return "stop", lo
        if tgt is not None and hi >= tgt:
            return "target", tgt
        if hi > hw:
            hw = hi
        if betr is not None and hw >= betr and stop < entry:
            stop = entry
    return ("timeflat", lc) if lc is not None else ("timeflat", entry)


def scan_recovery(day_bars, rh, rl):
    """Grammar-A breakdown→recovery scan. Returns (rec_ts, rec_close, breakdown_low) or None."""
    armed = False
    low_run = None
    for ts, hi, lo, cl in day_bars:
        tt = stamp(ts).time()
        if tt < REND:
            continue
        if tt >= FLAT:
            return None
        if not armed:
            if cl < rl:
                armed = True
                low_run = lo
            continue
        low_run = min(low_run, lo)
        if cl > rh:
            return None
        if cl > rl:
            return ts, cl, min(low_run, lo)
    return None


def baseline(perf, daily, minute):
    closed = [t for t in perf["trades"] if t.get("ts_closed") is not None]
    pnl = rc = 0.0
    rows = []
    for t in closed:
        q = t["quantity"]
        rcap = t.get("risk_capital")
        if rcap is None or q <= 0:
            continue
        sym = t["symbol"]
        sess = stamp(t["ts_opened"]).date()
        entry = round(t["avg_px_open"])
        rps = rcap / q
        a = atr(daily[sym], sess)
        wr = wratio(a, entry)
        rh, rl = range_bounds(minute[sym][sess])
        if rh is None:
            raise AssertionError(f"missing opening range for baseline {sym} {sess}")
        qr = qty(wr, rps, entry)
        if qr != int(q):
            raise AssertionError(f"baseline qty reconstruction mismatch {sym} {sess}: {qr} vs {int(q)}")
        ets = next(f["ts_event"] for f in t["fills"] if f["side"] == "BUY")
        reso, ex = walk(minute[sym][sess], entry, rh, rl, ets, entry - round(rps))
        pnl += qr * (ex - entry)
        rc += qr * max(1.0, rps)
        rows.append({"sym": sym, "sess": sess, "rh": rh, "rl": rl, "ts_closed": t["ts_closed"], "reso": reso})
    if not rows:
        raise AssertionError("no closed baseline trades with risk capital")
    return pnl, rc, rows


def score(cands, anchor_key, bp, brc, bror):
    pnl = rc = 0.0
    tally = {"stop": 0, "target": 0, "timeflat": 0}
    n = 0
    for c in cands:
        stop = c[anchor_key]
        rps = c["entry"] - stop
        if rps <= 0:
            continue
        q = qty(c["wr"], rps, c["entry"])
        if q <= 0:
            continue
        reso, ex = walk(c["bars"], c["entry"], c["rh"], c["rl"], c["rec_ts"], stop)
        pnl += q * (ex - c["entry"])
        rc += q * max(1.0, rps)
        tally[reso] += 1
        n += 1
    if n == 0:
        return 0, 0.0, 0.0, 0.0, 0.0
    shift = (bp + pnl) / (brc + rc) - bror
    return n, shift, tally["target"] / n, tally["stop"] / n, tally["timeflat"] / n


def main():
    out = sys.argv[-1]
    manifest = json.load(open(RUN_HOME / "manifest.json"))
    expect = {"run_id": RUN_ID, "strategy_id": "orb", "strategy_version": STRATEGY_VERSION,
              "strategy_code_hash": STRATEGY_HASH, "catalog_fingerprint": CATALOG_FINGERPRINT,
              "universe_hash": UNIVERSE_HASH}
    for k, v in expect.items():
        assert manifest.get(k) == v, f"head identity mismatch: {k}"
    assert catalog_fingerprint() == CATALOG_FINGERPRINT, "catalog content fingerprint mismatch"
    assert manifest.get("data_range") == {"start": DATA_START, "end": DATA_END}, "data-range mismatch"
    p = manifest["params"]
    assert p.get("stop_mode", 0.0) == 0.0, "not RangeLow"
    assert p.get("entry_confirm") == 1.0, "not close-confirm"
    assert p.get("range_minutes") == 20 and p.get("range_open") == "09:00:00", "range window drift"

    perf = json.load(open(RUN_HOME / "performance.json"))
    daily, minute = load_catalog()

    bp, brc, base_rows = baseline(perf, daily, minute)
    assert brc > 0, "undefined baseline RoR denominator"
    bror = bp / brc
    traded = {(t["symbol"], stamp(t["ts_opened"]).date()) for t in perf["trades"]}

    # ---- Grammar A: breakdown-recovery over the additive (never-traded) population ----
    a_cands = []
    for sym in sorted(minute):
        for sess, bars in minute[sym].items():
            if not (WIN_LO <= sess <= WIN_HI) or (sym, sess) in traded:
                continue
            book = daily.get(sym, {})
            if sess not in book:
                continue
            topen = book[sess][0]
            pri = [d for d in sorted(book) if d < sess]
            if not pri:
                continue
            pclose = book[pri[-1]][3]
            if pclose <= 0 or topen <= pclose:
                continue
            if (topen - pclose) / pclose * 100.0 < GAP_MIN:
                continue
            rh, rl = range_bounds(bars)
            if rh is None or rl <= 0:
                continue
            a = atr(book, sess)
            if ORW_MAX > 0.0 and a is not None and a > 0.0 and (rh - rl) > ORW_MAX * a:
                continue
            retain = (rl - pclose) / (topen - pclose)
            if not math.isfinite(retain) or retain > 1.0 or retain < RETAIN_MIN:
                continue
            rec = scan_recovery(bars, rh, rl)
            if rec is None or rh - rl <= 0:
                continue
            rec_ts, entry, blow = rec
            a_cands.append({"entry": entry, "rh": rh, "rl": rl, "breakdown_low": blow,
                            "rec_ts": rec_ts, "wr": wratio(a, entry), "bars": bars})
    a_anchor = {}
    for key in ("breakdown_low", "rl"):
        aid = ANCHOR_BREAKDOWN if key == "breakdown_low" else ANCHOR_RANGE
        a_anchor[aid] = (key, score(a_cands, key, bp, brc, bror))
    best = max(a_anchor, key=lambda aid: (a_anchor[aid][1][1], -aid))
    a_res = a_anchor[best][1]

    # ---- Grammar B: post-stop re-entry ----
    b_cands = []
    for r in base_rows:
        if r["reso"] != "stop":
            continue
        sym, sess = r["sym"], r["sess"]
        bars = minute[sym][sess]
        rh, rl = r["rh"], r["rl"]
        rec = None
        for ts, hi, lo, cl in bars:
            if ts <= r["ts_closed"]:
                continue
            if stamp(ts).time() >= FLAT:
                break
            if cl > rh:
                rec = None
                break
            if cl > rl:
                rec = (ts, cl)
                break
        if rec is None or rh - rl <= 0:
            continue
        rec_ts, entry = rec
        a = atr(daily[sym], sess)
        b_cands.append({"entry": entry, "rh": rh, "rl": rl, "rl_key": rl,
                        "rec_ts": rec_ts, "wr": wratio(a, entry), "bars": bars})
    b_res = score(b_cands, "rl_key", bp, brc, bror)

    grammars = {G_A: (a_res, best), G_B: (b_res, ANCHOR_RANGE)}

    def clears(res):
        return res[0] >= COUNT_FLOOR and res[1] >= ROR_FLOOR

    if clears(a_res):
        winner = G_A
    elif clears(b_res):
        winner = G_B
    else:
        winner = max(grammars, key=lambda gid: (grammars[gid][0][1], -gid))
    (wn, wshift, wtgt, wstop, wflat), wanchor = grammars[winner]

    print(f"twin: base RoR {bror:.4f} winner {int(winner)} anchor {int(wanchor)} "
          f"A(n={a_res[0]},shift={a_res[1]:+.6f}) B(n={b_res[0]},shift={b_res[1]:+.6f})")

    readings = {
        "population_count": float(wn),
        "ror_shift": round(wshift, 6),
        "resolution_target_share": round(wtgt, 4),
        "resolution_stop_share": round(wstop, 4),
        "resolution_timeflat_share": round(wflat, 4),
        "winning_grammar_id": float(winner),
        "stop_anchor_id": float(wanchor),
    }
    with open(out, "w") as fh:
        json.dump(readings, fh, sort_keys=True)


if __name__ == "__main__":
    main()
