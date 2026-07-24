#!/usr/bin/env python3
"""Independent twin of the max_concurrent slot-ranking Phase-A screen on v34 (plan 2026-07-24-002).

The GO/STOP verdict is load-bearing, so per the governed convention every gated reading is recomputed here
by a DIFFERENT code path from diagnostic.py, sharing no function. The independence is structural:

  * diagnostic.py loads each symbol's bars ENTRY-LOCALLY on demand; this twin PRELOADS the whole catalog
    into two dict-of-dict maps up front and indexes into them.
  * diagnostic.py carries book state as a list of per-holding dicts; this twin carries the book as a list of
    integer indices into PARALLEL ARRAYS of event fields (entry/stop/key/exit/eligibility), a deliberately
    different representation of the same Policy-D replay.

The barrier SEMANTICS are shared by design (RangeLow stop, decoupled 1.0R target / 0.41R breakeven ratchet,
close-confirm entry so no same-bar stop, resolution-bar-CLOSE fills matching the nautilus ledger, strict-rank
displacement with mark-to-market at the displacement bar close) — those are the model, not the bug surface.
diagnose STOPs on any disagreement beyond the frozen per-reading tolerance (population_count exact; the RoR
readings within 0.0005).
"""
import datetime as dt
import glob
import json
import os
import struct
import sys
from pathlib import Path

import pyarrow.parquet as pq

RUN_ID = "20260724T014752Z-backtest-orb-v34"
UNIT = 1_000_000_000
TZ = dt.timezone(dt.timedelta(hours=9))
WIN = 14
END_RANGE = dt.time(9, 20)
FLAT_T = dt.time(15, 0)
BUDGET_SLOTS = 7
PT_R = 1.0
BE_R = 0.41
CNT_FLOOR = 12
SHIFT_FLOOR = 0.005

ROOT = Path(__file__).resolve().parents[5]
DATA = ROOT / "data" / "turn4-fresh"
RUN_DIR = Path(os.environ.get("LS_SLOTRANK_RUN", str(DATA / "runs" / RUN_ID)))
CATALOG = DATA / "catalog" / "data" / "bars"


def fail(msg):
    raise SystemExit(f"FATAL(twin): {msg}")


def i64(blob):
    raw = struct.unpack("<q", blob)[0]
    if raw % UNIT != 0:
        fail("non-integral canonical price")
    return raw // UNIT


def when(ns):
    return dt.datetime.fromtimestamp(ns / UNIT, tz=dt.timezone.utc).astimezone(TZ)


def preload_catalog():
    """Whole-catalog preload: (daily, minute) where daily[sym][date]=(o,h,l,c) and
    minute[sym][date]=sorted[(ts,h,l,c)]. One pass over every parquet the catalog holds."""
    daily, minute = {}, {}
    for path in sorted(glob.glob(str(CATALOG / "*-1-DAY-LAST-EXTERNAL" / "*.parquet"))):
        sym = Path(path).parent.name.split("-1-DAY-")[0]
        cols = pq.read_table(path, columns=["open", "high", "low", "close", "ts_event"]).to_pydict()
        book = daily.setdefault(sym, {})
        latest = daily.setdefault("__ts__" + sym, {})
        for k in range(len(cols["ts_event"])):
            ts = cols["ts_event"][k]
            day = when(ts).date()
            if day not in latest or ts > latest[day]:
                latest[day] = ts
                book[day] = (i64(cols["open"][k]), i64(cols["high"][k]), i64(cols["low"][k]), i64(cols["close"][k]))
    for key in [k for k in daily if k.startswith("__ts__")]:
        del daily[key]
    for path in sorted(glob.glob(str(CATALOG / "*-1-MINUTE-LAST-EXTERNAL" / "*.parquet"))):
        sym = Path(path).parent.name.split("-1-MINUTE-")[0]
        cols = pq.read_table(path, columns=["high", "low", "close", "ts_event"]).to_pydict()
        smin = minute.setdefault(sym, {})
        for k in range(len(cols["ts_event"])):
            ts = cols["ts_event"][k]
            bar = (ts, i64(cols["high"][k]), i64(cols["low"][k]), i64(cols["close"][k]))
            smin.setdefault(when(ts).date(), {})[ts] = bar
    minute_sorted = {}
    for sym, days in minute.items():
        minute_sorted[sym] = {d: sorted(v.values(), key=lambda r: r[0]) for d, v in days.items()}
    return daily, minute_sorted


def atr_of(day_series, session):
    """prior_atr: strictly-prior sessions, need WIN+1, fail-closed to None (same formula as the head)."""
    prev = sorted(d for d in day_series if d < session)
    if len(prev) < WIN + 1:
        return None
    tail = prev[-(WIN + 1):]
    acc = 0.0
    for j in range(1, len(tail)):
        pc = day_series[tail[j - 1]][3]
        _o, h, l, _c = day_series[tail[j]]
        acc += max(h - l, abs(h - pc), abs(l - pc))
    return acc / WIN


def walk_exit(bars, entry, r_denom, after_ts, stop):
    """Return (exit_close, exit_ts): resolution mirrors the head (stop-first, target on r_denom, breakeven
    ratchet, timeflat); the FILL is the resolution bar's close. Written as an index walk over `bars`."""
    tgt = entry + round(PT_R * r_denom) if r_denom > 0 else None
    trig = entry + round(BE_R * r_denom) if r_denom > 0 else None
    peak = entry
    seen_ts = seen_close = None
    i = 0
    while i < len(bars):
        ts, hi, lo, cl = bars[i]
        i += 1
        if ts <= after_ts:
            continue
        seen_ts, seen_close = ts, cl
        if when(ts).time() >= FLAT_T:
            return cl, ts
        if lo <= stop:
            return cl, ts
        if tgt is not None and hi >= tgt:
            return cl, ts
        if hi > peak:
            peak = hi
        if trig is not None and peak >= trig and stop < entry:
            stop = entry
    if seen_ts is not None:
        return seen_close, seen_ts
    return entry, after_ts


def mtm(bars, at_ts):
    """Close at the displacement bar (bar at at_ts, else the latest earlier bar)."""
    pick = None
    for ts, _h, _l, cl in bars:
        if ts == at_ts:
            return cl
        if ts <= at_ts:
            pick = cl
        else:
            break
    if pick is None:
        fail(f"no displacement bar at/before {at_ts}")
    return pick


def main():
    out = sys.argv[-1]

    manifest = json.load(open(RUN_DIR / "manifest.json", encoding="utf-8"))
    if manifest.get("run_id") != RUN_ID or manifest.get("strategy_version") != 34:
        fail("head identity mismatch (run_id / strategy_version)")
    if manifest.get("catalog_fingerprint") != "363f199d4357bf665d3bed9c97c36e37551e24c89e89b0bad0b00de50d8908f4":
        fail("catalog fingerprint drift")

    # --- decisions: breakouts / placements / max_concurrent rejects ---
    bk, placed_keys, reject_qty = {}, set(), {}
    for line in open(RUN_DIR / "decisions.jsonl", encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        obj = json.loads(line)
        det = obj.get("decision_detail") or {}
        kind = det.get("kind")
        node = (det.get("symbol"), obj.get("ts_event"))
        if kind == "breakout":
            bk[node] = det["values"]
        elif kind == "order_placed":
            placed_keys.add(node)
        elif kind == "order_rejected_sizing" and det.get("filter") == "max_concurrent":
            reject_qty[node] = int(det["values"]["qty"])
    population_count = len(reject_qty)

    # --- performance: 119 closed trades keyed (sym, ts_opened) ---
    perf = json.load(open(RUN_DIR / "performance.json", encoding="utf-8"))
    closed_r, closed_rc, closed_q = {}, {}, {}
    for t in perf["trades"]:
        if t.get("ts_closed") is None or t.get("risk_capital") is None or t["quantity"] <= 0:
            continue
        node = (t["symbol"], t["ts_opened"])
        closed_r[node] = t["realized_r"]
        closed_rc[node] = t["risk_capital"]
        closed_q[node] = int(t["quantity"])

    daily, minute = preload_catalog()

    # --- parallel arrays over all breakouts, sorted by ts (the event stream) ---
    order = sorted(bk.keys(), key=lambda n: n[1])
    n = len(order)
    a_sym = [node[0] for node in order]
    a_ts = [node[1] for node in order]
    a_entry, a_rlow, a_rden, a_qty, a_key = [0]*n, [0]*n, [0]*n, [0]*n, [None]*n
    a_exitpx, a_exitts, a_elig, a_cohort_r, a_bars = [0]*n, [0]*n, [False]*n, [None]*n, [None]*n
    for k in range(n):
        node = order[k]
        v = bk[node]
        sym, ts = node
        entry = round(v["breakout_price"])
        rlow = round(v["range_low"])
        rden = round(v["range_high"]) - rlow
        a_entry[k], a_rlow[k], a_rden[k] = entry, rlow, rden
        is_placed = node in placed_keys
        is_reject = node in reject_qty
        if is_placed == is_reject:
            fail(f"breakout {node} neither/both placed and rejected")
        # qty only feeds RoR, so a placed breakout that is not one of the 119 closed (the 7 unreconciled +
        # 2 open) carries 0 — it occupies a slot but never books. A blocked breakout carries its reject qty.
        if is_placed:
            a_qty[k] = closed_q[node] if node in closed_r else 0
        else:
            a_qty[k] = reject_qty[node]
        sess = when(ts).date()
        atr = atr_of(daily.get(sym, {}), sess)
        a_key[k] = (rden / atr) if (atr and atr > 0 and rden > 0) else None
        bars = minute.get(sym, {}).get(sess, [])
        if not bars:
            fail(f"missing minute bars for {sym} {sess}")
        a_bars[k] = bars
        px, xts = walk_exit(bars, entry, rden, ts, rlow)
        a_exitpx[k], a_exitts[k] = px, xts
        a_elig[k] = (node in closed_r) or (not is_placed)
        a_cohort_r[k] = closed_r.get(node)

    # --- occupancy self-check: FIFO drop set must equal the 20 logged rejects ---
    live_exit = []
    dropped = set()
    for k in range(n):
        live_exit = [x for x in live_exit if x > a_ts[k]]
        if len(live_exit) < BUDGET_SLOTS:
            live_exit.append(a_exitts[k])
        else:
            dropped.add(order[k])
    if dropped != set(reject_qty.keys()):
        fail(f"FIFO occupancy replay dropped {len(dropped)} != 20 logged rejects")

    # --- ror_base: performance.json realized_r over the 119 closed ---
    base_num = sum(closed_rc[node] * closed_r[node] for node in closed_r)
    base_den = sum(closed_rc.values())
    ror_base = base_num / base_den

    # --- Policy-D ranked replay; book = list of event indices ---
    book = []                 # indices of live holdings
    live_exit_ts = {}         # index -> (possibly shortened) exit ts
    booked = {}               # index -> realized_r (RoR-eligible only)

    def natural_r(k):
        if a_cohort_r[k] is not None:
            return a_cohort_r[k]
        return (a_exitpx[k] - a_entry[k]) / (a_entry[k] - a_rlow[k])

    def book_if_eligible(k):
        if a_elig[k]:
            booked[k] = natural_r(k)

    for k in range(n):
        ts = a_ts[k]
        survivors = []
        for j in book:
            if live_exit_ts[j] > ts:
                survivors.append(j)
            else:
                book_if_eligible(j)
        book = survivors
        if len(book) > BUDGET_SLOTS:
            fail(f"ranked book exceeded {BUDGET_SLOTS} slots")

        if len(book) < BUDGET_SLOTS:
            book.append(k)
            live_exit_ts[k] = a_exitts[k]
            continue
        # full book: strict-rank displacement of the widest rankable held
        held_rankable = [j for j in book if a_key[j] is not None]
        if a_key[k] is not None and held_rankable:
            w = max(held_rankable, key=lambda j: (a_key[j], a_ts[j]))
            if a_key[w] > a_key[k]:
                if a_elig[w]:
                    booked[w] = (mtm(a_bars[w], ts) - a_entry[w]) / (a_entry[w] - a_rlow[w])
                book.remove(w)
                book.append(k)
                live_exit_ts[k] = a_exitts[k]
                continue
        # else: drop k (as today)
    for j in book:
        book_if_eligible(j)

    prime_num = prime_den = 0.0
    for k, r in booked.items():
        node = order[k]
        rc = closed_rc[node] if a_cohort_r[k] is not None else a_qty[k] * max(1.0, a_entry[k] - a_rlow[k])
        prime_num += rc * r
        prime_den += rc
    ror_prime = prime_num / prime_den
    ror_shift = ror_prime - ror_base

    print(f"twin: population_count={population_count}  ror_base={ror_base:.6f}  "
          f"ror_prime={ror_prime:.6f}  ror_shift={ror_shift:+.6f}")
    print(f"twin verdict: count {'PASS' if population_count >= CNT_FLOOR else 'STOP'}  "
          f"shift {'PASS' if ror_shift >= SHIFT_FLOOR else 'STOP'}")

    readings = {
        "population_count": float(population_count),
        "ror_base": round(ror_base, 6),
        "ror_prime": round(ror_prime, 6),
        "ror_shift": round(ror_shift, 6),
    }
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(readings, fh, sort_keys=True)


if __name__ == "__main__":
    main()
