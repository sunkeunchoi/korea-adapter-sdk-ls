#!/usr/bin/env python3
"""Phase-A slot-reallocation screen for max_concurrent RANK-AWARE admission on v34 (plan 2026-07-24-002).

A REALLOCATION lever, not a sizing or entry-filter one. Head v34 fills its scarce `max_concurrent = 7`
slots first-come-first-served by breakout time (`sizing_allows(open) = open_positions < max_concurrent`,
params.rs:875). This screen asks whether re-ranking WHICH breakouts win those slots — by opening-range
tightness `range_R / prior_ATR` (narrower ranks higher) — would raise the head's size-invariant
return-on-risk. It writes no strategy code; it re-simulates v34's own slot allocation under two policies
and emits an honest GO/STOP reading.

The mechanism is DISPLACEMENT (Policy D): a full book may drop its widest-ranked open position to admit a
strictly-tighter new breakout, booking the displaced position mark-to-market at the displacement bar's
close (KTD8 — the screen pays the displacement cost, so ror_shift is conservative). Unrankable breakouts
(a symbol with < 15 prior daily sessions near the 2026-05-18 catalog start — KTD4) keep exact base/FIFO
behaviour: admitted if a slot is free, dropped if full, never displacing and never displaced.

Readings (mirrored in candidate.json, the machine home):
  population_count = number of eligible breakouts the max_concurrent budget blocked over v34 (exact int)
  ror_base        = size-invariant RoR of v34's 119 closed trades = sum(rc*r)/sum(rc)   (== 0.039806)
  ror_prime       = size-invariant RoR of the ranked (Policy-D) book, one consistent fill model
  ror_shift       = ror_prime - ror_base   (SIGNED — the load-bearing additive reading)

Frozen additive/reallocation gate (R8; no collinearity sub-gate — a slot reallocation has no per-trade
weight vector to correlate against):
  population_count >= 12       (a thin blocked cohort is a NO-BUILD regardless of shift)
  ror_shift        >= 0.005    (the additive-family floor, below the smallest kept gain ratio-ATR +0.0091)

Count reconciliation (KTD2): v34's decisions.jsonl logs 128 `order_placed` events but performance.json
reports 119 closed trades (+ 2 open at backtest end + 7 placements that never reconcile to a trade). All
128 placements OCCUPY a max_concurrent slot for their lifetime; only the 119 closed carry a realized RoR.
The re-sim therefore models book occupancy from all 128 placements (a FIFO replay reproduces exactly the
20 logged max_concurrent rejects — an occupancy self-check) while scoring RoR only over the 119 closed.

One consistent fill model (R7): the base book's undisplaced closed trades keep their performance.json
realized_r; every CHANGED position (a closed trade booked early by displacement, or a previously-blocked
breakout now admitted) is scored by this script's own bar engine. The engine is validated first: it
reproduces v34's 119 closed trades' realized_r within tolerance (KTD2 self-check) before it scores any
blocked or displaced trade. So the two books differ only by allocation, not by fill bias.

Structural independence (KTD1/KTD4): this script reconstructs ENTRY-LOCALLY (each symbol loads its own
bars on demand); twin.py builds catalog-wide maps with parallel-array book state and no shared function.
The barrier SEMANTICS (RangeLow stop, decoupled 1.0R target / 0.41R breakeven ratchet, close-confirm entry,
stop-first pessimism) are shared by design — the independence is in reconstruction and statistics.
"""
import datetime as dt
import glob
import hashlib
import json
import math
import os
import struct
import sys
from pathlib import Path

import pyarrow.parquet as pq

# ---- frozen head identity (v34) ----
RUN_ID = "20260724T014752Z-backtest-orb-v34"
STRATEGY_VERSION = 34
STRATEGY_HASH = "d7a9820b7356547ac8de0d0b8b11748dea6e83be7168744ef6591a88ce31145e"
CATALOG_FINGERPRINT = "363f199d4357bf665d3bed9c97c36e37551e24c89e89b0bad0b00de50d8908f4"
UNIVERSE_HASH = "13438ce8128a19a44f7d0cf17037d825b5388d4c4ba114630da655b0b06aba71"
DATA_START, DATA_END = "20260518", "20260722"

REPO_ROOT = Path(__file__).resolve().parents[5]
DATA_HOME = REPO_ROOT / "data" / "turn4-fresh"
# LS_SLOTRANK_RUN overrides the run DIR for the fixture harness only (KTD3); diagnose never sets it,
# so the frozen v34 path is what the gate reads. BARS_HOME/fingerprint stay pinned to the real catalog.
RUN_HOME = Path(os.environ.get("LS_SLOTRANK_RUN", str(DATA_HOME / "runs" / RUN_ID)))
BARS_HOME = DATA_HOME / "catalog" / "data" / "bars"

# ---- frozen v34 params (manifest, not source defaults) ----
SCALE = 1_000_000_000
KST = dt.timezone(dt.timedelta(hours=9))
ATR_WINDOW = 14
RANGE_END = dt.time(9, 20)          # range_open 09:00 + range_minutes 20
FLAT = dt.time(15, 0)
MAX_CONCURRENT = 7
PROFIT_TARGET_R = 1.0
BREAKEVEN_TRIGGER_R = 0.41

# ---- frozen screen pre-register (candidate.json is the machine home; this mirrors it) ----
COUNT_FLOOR = 12
ROR_SHIFT_FLOOR = 0.005
CALIB_ROR_TOL = 0.005       # bar-engine aggregate ror_base must land within this of performance.json's 0.039806
CALIB_PX_MATCH_MIN = 0.90   # >= this fraction of closed trades must fill at exactly the bar close the engine picks


def require(cond, msg):
    if not cond:
        raise RuntimeError(msg)


def canonical_price(raw):
    """Integer KRW/tick canonical price (the strategy's i64 representation)."""
    value = struct.unpack("<q", raw)[0]
    require(value % SCALE == 0, "non-integral canonical KRW/tick price")
    return value // SCALE


def kst_stamp(ns):
    return dt.datetime.fromtimestamp(ns / SCALE, tz=dt.timezone.utc).astimezone(KST)


def kst_date(ns):
    return kst_stamp(ns).date()


def display_fixed(raw, precision):
    value = struct.unpack("<q", raw)[0]
    sign = "-" if value < 0 else ""
    value = abs(value)
    whole, fraction = divmod(value, SCALE)
    if precision == 0:
        return f"{sign}{whole}"
    fraction //= 10 ** (9 - precision)
    return f"{sign}{whole}.{fraction:0{precision}d}"


def actual_catalog_fingerprint():
    """Re-derive the catalog content fingerprint over the frozen range (identity guard, KTD3)."""
    start_ns = int(dt.datetime(2026, 5, 18, tzinfo=KST).timestamp()) * SCALE
    end_ns = int(dt.datetime(2026, 7, 23, tzinfo=KST).timestamp()) * SCALE - 1
    unique = set()
    for file_name in sorted(BARS_HOME.glob("*/*.parquet")):
        table = pq.read_table(file_name, columns=["open", "high", "low", "close", "volume", "ts_event"])
        metadata = table.schema.metadata or {}
        bar_type = metadata.get(b"bar_type", b"").decode()
        price_precision = int(metadata.get(b"price_precision", b"-1"))
        size_precision = int(metadata.get(b"size_precision", b"-1"))
        require(bar_type and 0 <= price_precision <= 9 and 0 <= size_precision <= 9, f"invalid Parquet metadata: {file_name}")
        values = table.to_pydict()
        for index, event in enumerate(values["ts_event"]):
            if start_ns <= event <= end_ns:
                unique.add((
                    bar_type, event,
                    values["open"][index], values["high"][index],
                    values["low"][index], values["close"][index],
                    values["volume"][index], price_precision, size_precision,
                ))
    require(unique, "catalog has no bars in the frozen range")
    lines = []
    for bar_type, event, open_, high, low, close, volume, price_precision, size_precision in unique:
        prices = [display_fixed(value, price_precision) for value in (open_, high, low, close)]
        lines.append("|".join([bar_type, str(event), *prices, display_fixed(volume, size_precision)]))
    digest = hashlib.sha256()
    for line in sorted(lines):
        digest.update(line.encode())
        digest.update(b"\n")
    return digest.hexdigest()


# ---- entry-local bar loaders (each symbol loads its own bars on demand) ----

def daily_map(symbol):
    """{session_date: (open, high, low, close)} in integer KRW, latest bar per date wins."""
    files = sorted(glob.glob(str(BARS_HOME / f"{symbol}-1-DAY-LAST-EXTERNAL" / "*.parquet")))
    require(files, f"missing daily bars for {symbol}")
    latest_ts, out = {}, {}
    for file_name in files:
        table = pq.read_table(file_name, columns=["open", "high", "low", "close", "ts_event"]).to_pydict()
        for i in range(len(table["ts_event"])):
            ts = table["ts_event"][i]
            day = kst_date(ts)
            if day not in latest_ts or ts > latest_ts[day]:
                latest_ts[day] = ts
                out[day] = (
                    canonical_price(table["open"][i]), canonical_price(table["high"][i]),
                    canonical_price(table["low"][i]), canonical_price(table["close"][i]),
                )
    return out


def minute_sessions(symbol):
    """{session_date: sorted [(ts, high, low, close)]} for the symbol, in integer KRW.

    A divergent duplicate minute bar (two rows at the same ts_event disagreeing on OHLC) ABORTS; an exact
    duplicate is kept once so it is never walked twice.
    """
    files = sorted(glob.glob(str(BARS_HOME / f"{symbol}-1-MINUTE-LAST-EXTERNAL" / "*.parquet")))
    require(files, f"missing minute bars for {symbol}")
    sessions, seen = {}, {}
    for file_name in files:
        table = pq.read_table(file_name, columns=["high", "low", "close", "ts_event"]).to_pydict()
        for i in range(len(table["ts_event"])):
            ts = table["ts_event"][i]
            bar = (canonical_price(table["high"][i]), canonical_price(table["low"][i]), canonical_price(table["close"][i]))
            if ts in seen:
                require(seen[ts] == bar, f"divergent duplicate minute bar for {symbol} at ts {ts}")
                continue
            seen[ts] = bar
            sessions.setdefault(kst_date(ts), []).append((ts, *bar))
    for day in sessions:
        sessions[day].sort(key=lambda r: r[0])
    return sessions


def prior_atr(dmap, session, window=ATR_WINDOW):
    """Exact port of backtest.rs::prior_atr — strictly-prior sessions, require window+1 priors, fail-closed."""
    priors = sorted((d, dmap[d]) for d in dmap if d < session)
    if len(priors) < window + 1:
        return None
    tail = priors[-(window + 1):]
    sum_tr = 0.0
    for k in range(1, len(tail)):
        pc = tail[k - 1][1][3]
        _, h, l, _ = tail[k][1]
        sum_tr += max(h - l, abs(h - pc), abs(l - pc))
    return sum_tr / window


def simulate(bars, entry, r_denom, entry_ts, stop):
    """(resolution, exit_price, exit_ts) for a long entered at `entry` with initial `stop`.

    Resolution GEOMETRY mirrors orb.rs on_bar exactly (RangeLow-decoupled 1.0R target / 0.41R breakeven
    ratchet on r_denom = range_high - range_low; close-confirm entry -> walk bars strictly AFTER the entry
    bar so no same-bar stop; stop-first pessimism when a bar breaches both). The FILL PRICE, however, is the
    resolution bar's CLOSE for every reason — the marketable-limit Exit order (`limit_price: low`/`target` in
    orb.rs) is matched at the bar close in the nautilus backtest ledger that writes performance.json, not at
    the wick. This reproduces v34's realized fills (a stop bar that dips to the range low then recovers books
    near the recovered close, not the low), validated by `calibrate`. The detection bar (hence exit_ts and
    slot-free time) is unchanged by the fill-price choice.
    """
    target = entry + round(PROFIT_TARGET_R * r_denom) if r_denom > 0 else None
    be_trigger = entry + round(BREAKEVEN_TRIGGER_R * r_denom) if r_denom > 0 else None
    high_water = entry
    last_ts = last_close = None
    for ts, high, low, close in bars:
        if ts <= entry_ts:
            continue
        last_ts, last_close = ts, close
        if kst_stamp(ts).time() >= FLAT:
            return "timeflat", close, ts
        if low <= stop:
            return "stop", close, ts
        if target is not None and high >= target:
            return "target", close, ts
        high_water = max(high_water, high)
        if be_trigger is not None and high_water >= be_trigger:
            stop = max(stop, entry)
    if last_ts is not None:
        return "timeflat", last_close, last_ts
    return "timeflat", entry, entry_ts


def mtm_close(bars, at_ts):
    """Mark-to-market close at the displacement bar: the close of the bar at `at_ts`, else the latest
    bar with ts <= at_ts (bars are ts-sorted). Displacement is always intra-session, after the position's
    own entry bar, so a prior bar always exists."""
    chosen = None
    for ts, _high, _low, close in bars:
        if ts == at_ts:
            return close
        if ts <= at_ts:
            chosen = close
        else:
            break
    require(chosen is not None, f"no bar at or before displacement ts {at_ts}")
    return chosen


# ---------------------------------------------------------------------------
# load v34 decisions + performance
# ---------------------------------------------------------------------------

def load_decisions(path):
    """Return (breakouts, placed, rejects_mc). Each keyed on (symbol, ts_event); one breakout per key."""
    breakouts, placed, rejects_mc = {}, {}, {}
    for raw in open(path, encoding="utf-8"):
        raw = raw.strip()
        if not raw:
            continue
        env = json.loads(raw)
        ts = env["ts_event"]
        det = env.get("decision_detail") or {}
        kind = det.get("kind")
        sym = det.get("symbol")
        if kind == "breakout":
            key = (sym, ts)
            require(key not in breakouts, f"duplicate breakout envelope for {key}")
            breakouts[key] = det["values"]
        elif kind == "order_placed":
            placed[(sym, ts)] = det["values"]
        elif kind == "order_rejected_sizing" and det.get("filter") == "max_concurrent":
            rejects_mc[(sym, ts)] = det.get("values") or {}
    return breakouts, placed, rejects_mc


def load_closed(path):
    """Closed trades keyed on (symbol, ts_opened) -> record; risk_capital present and quantity>0."""
    perf = json.load(open(path, encoding="utf-8"))
    closed = {}
    for t in perf["trades"]:
        if t.get("ts_closed") is None:
            continue
        rc = t.get("risk_capital")
        if rc is None or t["quantity"] <= 0:
            continue
        closed[(t["symbol"], t["ts_opened"])] = {
            "qty": int(t["quantity"]),
            "entry": round(t["avg_px_open"]),
            "exit": round(t["avg_px_close"]),
            "rc": rc,
            "realized_r": t["realized_r"],
        }
    return closed, perf


def assert_identity(manifest):
    expected = {
        "run_id": RUN_ID, "strategy_id": "orb", "strategy_version": STRATEGY_VERSION,
        "strategy_code_hash": STRATEGY_HASH, "catalog_fingerprint": CATALOG_FINGERPRINT,
        "universe_hash": UNIVERSE_HASH,
    }
    for name, value in expected.items():
        require(manifest.get(name) == value, f"head identity mismatch: {name}")
    require(manifest.get("data_range") == {"start": DATA_START, "end": DATA_END}, "data-range mismatch")
    params = manifest.get("params", {})
    require(params.get("stop_mode", 0.0) == 0.0, "head is not RangeLow (stop_mode != 0) — screen premise broken")
    require(params.get("entry_confirm") == 1.0, "head is not close-confirm (entry_confirm != 1)")
    require(params.get("max_concurrent") == float(MAX_CONCURRENT), "max_concurrent drift")
    require(params.get("profit_target_r") == PROFIT_TARGET_R, "profit_target_r drift")
    require(params.get("breakeven_trigger_r") == BREAKEVEN_TRIGGER_R, "breakeven_trigger_r drift")
    require(actual_catalog_fingerprint() == CATALOG_FINGERPRINT, "catalog content fingerprint mismatch")


# ---------------------------------------------------------------------------
# breakout records: entry / stop / rank key / qty / bar-engine natural exit
# ---------------------------------------------------------------------------

def build_records(breakouts, placed, rejects_mc, closed):
    """One record per breakout (placed or blocked), carrying everything the two replays need.

    key = range_R / prior_ATR (ascending = tighter); None when unrankable (KTD4). natural exit is the bar
    engine's stop/target/timeflat resolution. `booked` marks the 119-closed RoR cohort; the 7 unreconciled
    + 2 open placements occupy slots but never enter RoR.
    """
    dmap_cache, mins_cache = {}, {}
    records = {}
    for (sym, ts), vals in breakouts.items():
        entry = round(vals["breakout_price"])
        range_high = round(vals["range_high"])
        range_low = round(vals["range_low"])
        r_denom = range_high - range_low
        rps = entry - range_low                       # RangeLow per-share risk (stop_mode 0)
        is_placed = (sym, ts) in placed
        is_reject = (sym, ts) in rejects_mc
        require(is_placed != is_reject, f"breakout {sym}@{ts} is neither/both placed and max_concurrent-rejected")
        if is_placed:
            qty = int(placed[(sym, ts)]["qty"])
        else:
            qty = int(rejects_mc[(sym, ts)]["qty"])
        if sym not in dmap_cache:
            dmap_cache[sym] = daily_map(sym)
            mins_cache[sym] = minute_sessions(sym)
        sess = kst_date(ts)
        atr = prior_atr(dmap_cache[sym], sess)
        key = (r_denom / atr) if (atr is not None and atr > 0 and r_denom > 0) else None
        bars = mins_cache[sym].get(sess, [])
        require(bars, f"missing minute session bars for {sym} {sess}")
        stop = range_low
        reso, exit_px, exit_ts = simulate(bars, entry, r_denom, ts, stop)
        cohort = closed.get((sym, ts))
        # RoR-cohort eligibility: the 119 closed placements (perf-scored) and the 20 blocked breakouts
        # (engine-scored once admitted). The 7 unreconciled + 2 open placements occupy slots but never book
        # into RoR, in either policy — so they carry rc_eligible = False.
        rc_eligible = (cohort is not None) or (not is_placed)
        records[(sym, ts)] = {
            "sym": sym, "ts": ts, "sess": sess, "entry": entry, "range_low": range_low,
            "r_denom": r_denom, "rps": rps, "qty": qty, "key": key, "bars": bars,
            "placed": is_placed, "resolution": reso, "exit_px": exit_px, "exit_ts": exit_ts,
            "rc_eligible": rc_eligible, "cohort": cohort,
        }
    return records


def calibrate(records, closed, ror_base):
    """KTD2 self-check: the bar engine reproduces v34's 119 closed trades before it scores any blocked trade.

    Two checks, both aggregate-safe. (1) The engine's own size-invariant ror_base — scored on the RangeLow
    per-share risk (entry - range_low, the recorded risk_capital/qty) with the resolution-bar-close fill —
    must land within CALIB_ROR_TOL of performance.json's 0.039806. (2) At least CALIB_PX_MATCH_MIN of the
    119 closed trades must fill at exactly the bar close the engine resolves on (an exact per-trade fill
    match). A blocked trade is scored by the same engine, so passing both bounds means the base->blocked
    hand-off carries no material fill bias (R7); any residual is a conservative pessimism on ror_shift.
    """
    require(len(closed) == 119, f"expected 119 closed trades, got {len(closed)}")
    num = den = 0.0
    px_match = 0
    worst = 0.0
    for (sym, ts), c in closed.items():
        rec = records.get((sym, ts))
        require(rec is not None, f"closed trade {sym}@{ts} has no breakout envelope")
        rps = rec["entry"] - rec["range_low"]         # RangeLow per-share risk (== recorded risk_capital/qty)
        reso, exit_px, _ = simulate(rec["bars"], rec["entry"], rec["r_denom"], ts, rec["range_low"])
        r_engine = (exit_px - rec["entry"]) / rps
        num += c["qty"] * rps * r_engine
        den += c["qty"] * rps
        if exit_px == c["exit"]:
            px_match += 1
        worst = max(worst, abs(r_engine - c["realized_r"]))
    engine_ror = num / den
    frac_px = px_match / len(closed)
    require(
        abs(engine_ror - ror_base) <= CALIB_ROR_TOL,
        f"bar-engine calibration failed: engine ror_base {engine_ror:.6f} is > {CALIB_ROR_TOL} from "
        f"performance.json {ror_base:.6f} — the exit engine does not reproduce v34",
    )
    require(
        frac_px >= CALIB_PX_MATCH_MIN,
        f"bar-engine calibration failed: only {px_match}/{len(closed)} closed trades fill at the resolved "
        f"bar close (< {CALIB_PX_MATCH_MIN:.0%}) — the fill model does not reproduce v34",
    )
    return engine_ror, px_match, worst


def fifo_occupancy_check(records, rejects_mc):
    """Occupancy self-check: a FIFO replay over all breakouts (bar-engine natural exits) must drop EXACTLY
    the 20 logged max_concurrent rejects. Validates the slot-occupancy model end to end."""
    events = sorted(records.values(), key=lambda r: r["ts"])
    book = []            # list of exit_ts of open positions
    dropped = set()
    for e in events:
        book = [x for x in book if x > e["ts"]]        # free slots that exited at/before this ts
        if len(book) < MAX_CONCURRENT:
            book.append(e["exit_ts"])
        else:
            dropped.add((e["sym"], e["ts"]))
    require(
        dropped == set(rejects_mc.keys()),
        f"FIFO occupancy replay dropped {len(dropped)} breakouts, not the 20 logged max_concurrent "
        f"rejects (real-only {len(set(rejects_mc) - dropped)}, model-only {len(dropped - set(rejects_mc))})",
    )


def ranked_book(records):
    """Policy-D replay: displacement of the widest-key held position by a strictly-tighter new breakout.

    Returns {(sym,ts): booked_realized_r} for every position that ends up booked in the ranked cohort.
    A booked position's realized_r is: performance.json realized_r if it is an undisplaced closed trade;
    the bar-engine (exit-entry)/rps if it is a newly-admitted blocked breakout; or the mark-to-market
    (mtm_close-entry)/rps if it was displaced (closed or blocked). The 7 unreconciled + 2 open placements
    occupy slots but are never booked into RoR.
    """
    events = sorted(records.values(), key=lambda r: r["ts"])
    book = []            # live holdings: dicts with key/exit_ts/rec
    outcome = {}         # (sym,ts) -> realized_r (only RoR-eligible booked positions)

    def realized_natural(rec):
        if rec["cohort"] is not None:                  # undisplaced closed -> performance.json truth
            return rec["cohort"]["realized_r"]
        return (rec["exit_px"] - rec["entry"]) / rec["rps"]   # newly-admitted blocked -> bar engine (close fill)

    def book_out(rec):
        if rec["rc_eligible"]:
            outcome[(rec["sym"], rec["ts"])] = realized_natural(rec)

    for e in events:
        # free slots whose (possibly displacement-shortened) exit is at/before this ts — a freed position
        # that reached its natural exit books its natural outcome.
        keep = []
        for h in book:
            if h["exit_ts"] > e["ts"]:
                keep.append(h)
            else:
                book_out(h["rec"])
        book = keep
        require(len(book) <= MAX_CONCURRENT, f"ranked book exceeded {MAX_CONCURRENT} slots before {e['ts']}")

        if len(book) < MAX_CONCURRENT:
            book.append({"key": e["key"], "exit_ts": e["exit_ts"], "rec": e})
            continue
        # full book: displacement only when the new breakout is rankable and strictly tighter than the
        # widest rankable held position (unrankable held can never be displaced — KTD4).
        rankable = [h for h in book if h["key"] is not None]
        if e["key"] is not None and rankable:
            widest = max(rankable, key=lambda h: (h["key"], h["rec"]["ts"]))
            if widest["key"] > e["key"]:
                w = widest["rec"]
                if w["rc_eligible"]:                    # book the displaced position mark-to-market at e.ts
                    mtm = mtm_close(w["bars"], e["ts"])
                    outcome[(w["sym"], w["ts"])] = (mtm - w["entry"]) / w["rps"]
                book.remove(widest)
                book.append({"key": e["key"], "exit_ts": e["exit_ts"], "rec": e})
                continue
        # otherwise: drop e (as today) — never booked
    # flush the residual book at end of stream (positions that ran to natural exit)
    for h in book:
        book_out(h["rec"])
    return outcome


def size_invariant_ror(members):
    """sum(rc * r) / sum(rc) over (rc, realized_r) members."""
    num = sum(rc * r for rc, r in members)
    den = sum(rc for rc, _ in members)
    require(den > 0, "undefined RoR denominator")
    return num / den


def main():
    require(len(sys.argv) >= 2, "missing readings output path")
    out_path = sys.argv[-1]

    manifest = json.load(open(RUN_HOME / "manifest.json", encoding="utf-8"))
    assert_identity(manifest)

    breakouts, placed, rejects_mc = load_decisions(RUN_HOME / "decisions.jsonl")
    closed, _perf = load_closed(RUN_HOME / "performance.json")

    require(len(breakouts) == len(placed) + len(rejects_mc),
            f"breakout count {len(breakouts)} != placed {len(placed)} + rejects {len(rejects_mc)}")
    for key in rejects_mc:
        require(key in breakouts, f"max_concurrent reject {key} has no breakout envelope")

    records = build_records(breakouts, placed, rejects_mc, closed)

    population_count = len(rejects_mc)

    # ror_base: v34's 119 closed trades, performance.json realized_r (the anchor cohort, the 0.039806 head).
    base_members = [(c["rc"], c["realized_r"]) for c in closed.values()]
    ror_base = size_invariant_ror(base_members)

    engine_ror, px_match, worst = calibrate(records, closed, ror_base)
    fifo_occupancy_check(records, rejects_mc)

    # ror_prime: the ranked (Policy-D) book, one consistent fill model.
    outcome = ranked_book(records)
    prime_members = []
    for (sym, ts), r in outcome.items():
        rec = records[(sym, ts)]
        if rec["cohort"] is not None:                  # closed trade: keep its recorded risk capital
            rc = rec["cohort"]["rc"]
        else:                                          # newly-admitted blocked breakout
            rc = rec["qty"] * max(1.0, rec["rps"])
        prime_members.append((rc, r))
    ror_prime = size_invariant_ror(prime_members)
    ror_shift = ror_prime - ror_base

    # --- book composition telemetry (stdout only; the gate reads the JSON file) ---
    rankable = sum(1 for r in records.values() if r["key"] is not None)
    blocked_admitted = sum(1 for (s, t) in outcome if not records[(s, t)]["placed"])
    displaced = sum(
        1 for (s, t), r in outcome.items()
        if records[(s, t)]["cohort"] is not None and abs(r - records[(s, t)]["cohort"]["realized_r"]) > 1e-12
    )
    print(f"run: {RUN_HOME}")
    print(f"breakouts: {len(breakouts)}  placed: {len(placed)}  blocked(max_concurrent): {population_count}")
    print(f"rankable breakouts: {rankable}/{len(breakouts)}  (unrankable keep FIFO behaviour — KTD4)")
    print(f"calibration: engine ror_base {engine_ror:.6f} vs perf {ror_base:.6f}; "
          f"{px_match}/119 exact fills (worst |dr| {worst:.4f})")
    print(f"ranked book: {len(prime_members)} booked  (blocked admitted {blocked_admitted}, "
          f"closed displaced {displaced})")
    print()
    print(f"population_count: {population_count}   >= {COUNT_FLOOR} -> {'PASS' if population_count >= COUNT_FLOOR else 'STOP'}")
    print(f"ror_base:  {ror_base:.6f}")
    print(f"ror_prime: {ror_prime:.6f}")
    print(f"ror_shift: {ror_shift:+.6f}   >= {ROR_SHIFT_FLOOR} -> {'PASS' if ror_shift >= ROR_SHIFT_FLOOR else 'STOP'}")
    go = population_count >= COUNT_FLOOR and ror_shift >= ROR_SHIFT_FLOOR
    print()
    print(f"=== PHASE-A DECISION: {'GO' if go else 'STOP (NO-BUILD)'} ===")

    readings = {
        "population_count": float(population_count),
        "ror_base": round(ror_base, 6),
        "ror_prime": round(ror_prime, 6),
        "ror_shift": round(ror_shift, 6),
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(readings, fh, sort_keys=True)


if __name__ == "__main__":
    main()
