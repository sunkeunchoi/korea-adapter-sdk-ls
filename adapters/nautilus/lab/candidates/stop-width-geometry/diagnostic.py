#!/usr/bin/env python3
"""Phase-A four-signal screen for a stop-WIDTH-geometry conditioning lever on v32 (plan 2026-07-21-001).

Turn 11 tests whether the initial stop *location* — not stop-as-risk-sizing — carries
independent edge in the ORB loop. CLASS B sizing (`qty = budget·w_ratio / risk_per_share`)
already owns the risk axis: re-scale the stop by any factor `w` and `qty` re-sizes inversely,
so `risk_capital = qty·(w·rps)` stays pinned at the budget. A stop re-scale is INVISIBLE to
the RoR denominator; the only surviving effect is **stop-out geometry** — which fraction of
trades resolve stop / target / timeout. Independent edge can therefore live only in a
conditioning signal DECORRELATED from the two KEPT risk levers (`risk_per_share`, the ratio-ATR
weight) that predicts *when* more or less stop room pays.

The screen evaluates FOUR conditioning signals over v32 head's closed trades, forms each one's
candidate stop-width weight `w = clamp((ref/signal)^alpha, w_lo, w_hi)` (the ratio-ATR tilt
form), and gates each on:

  Gate 1a (collinearity vs the risk axis):  |Pearson r(w, risk_per_share)| < 0.70
  Gate 1b (collinearity vs the KEPT tilt):  |Pearson r(w, w_ratio_atr)|   < 0.70
  Gate 2a (RoR materiality, ceiling-aware):  projected ror_shift          >= ROR_SHIFT_FLOOR
  Gate 2b (geometry materiality):            resolution_mix_shift         >= RES_MIX_FLOOR

The WINNER is selected INSIDE this script (KTD7 — the gate contract is single-signal): among
signals clearing all four gates, the largest projected RoR-shift; if NONE clears, the
best-by-ror_shift among all four (its readings then fail a threshold, so the tool records STOP
= NO-BUILD). The winner's four canonical readings go into `readings.json` (the block the gate
checks), plus `winning_signal_id` (1..4, tolerance 0 — the twin must independently agree on the
winner) and the winner's companion seeds `stop_width_ref` / `stop_width_w_lo` / `stop_width_w_hi`
as extra numeric keys that flow into the verdict's `diagnostic_readings` (U5/U6 read them).

STOP-MODE READ (R6, hard prerequisite). v32 head runs `stop_mode = 0.0` = RangeLow: the stop
sits at the opening-range low and `r_denom = range_high − range_low` (OR-width) is DECOUPLED
from the stop. So a stop-width weight moves the stop (and `risk_per_share`) but leaves the
target/breakeven fixed — it changes reward:risk, NOT barrier-scaling. The screen asserts this
mode from the manifest and simulates barriers on that basis (AE4).

MATERIALITY MODEL (KTD3). `ror_shift` and `resolution_mix_shift` are read from an offline
geometry re-simulation of each closed trade over its own catalog minute bars (RangeLow,
close-confirm entry so no same-bar stop, stop-first pessimism, breakeven ratchet to entry from
the next bar). For each candidate weight the stop distance scales to `w·rps` (floored at 1 tick),
`qty` re-sizes via CLASS B, and the first barrier the price path crosses decides the resolution:

  * resolution_mix_shift = fraction of trades whose stop/target/timeout class differs from the
    w=1 simulation. It is FILL-PRICE-INDEPENDENT (pure geometry) — the primary materiality gate.
  * ror_shift = RoR(w) − RoR(w=1), both under the same pessimistic-fill model. Sizing (qty) is
    reconstructed EXACTLY (0/77 vs the run), so the whole sim(w=1) RoR ≈ 0.152 vs run 0.1876 gap
    is the favorable gap-through-limit fills the run books and this screen does not. The bias
    cancels only to FIRST order: the lever's mechanism is to convert a trade's resolution class,
    and a stop/timeout→target conversion earns that omitted premium in reality but only the flat
    target here — so the shift carries a residual DOWNWARD bias over the ≤ resolution_mix_shift·N
    converted trades (bounded ~0.0016 here, toward NO-BUILD). ror_shift is therefore a
    conservative lower bound on the true improvement, not an unbiased estimate.

All four signals passing Gate 1 with near-inert materiality is the honest CLASS-B-absorption
result the plan anticipated (the stop re-scale is auto-absorbed on the risk axis; the modest
governed stop move barely changes which barrier is hit first).
"""
import datetime
import glob
import json
import math
import struct
import sys
from pathlib import Path

import pyarrow.parquet as pq

# ---- frozen head identity (v32) ----
RUN_ID = "20260717T094841Z-backtest-orb-v32"
STRATEGY_VERSION = 32
STRATEGY_HASH = "d7a9820b7356547ac8de0d0b8b11748dea6e83be7168744ef6591a88ce31145e"
CATALOG_FINGERPRINT = "3b6be31bdf8d29a8d774d42d490020d65753455acfc1b2214a0b13f14b589200"
UNIVERSE_HASH = "1e7394ec17d880de86075178305569fb9769ff3b1c025c904e17f53af60035e1"
DATA_START, DATA_END = "20260526", "20260703"

REPO_ROOT = Path(__file__).resolve().parents[5]
DATA_HOME = REPO_ROOT / "data" / "turn4-fresh"
RUN_HOME = DATA_HOME / "runs" / RUN_ID
CATALOG = DATA_HOME / "catalog" / "data" / "bars"

# ---- frozen v32 params (manifest, not source defaults) ----
SCALE = 1e9
KST = datetime.timedelta(hours=9)
ATR_WINDOW = 14
RANGE_OPEN = datetime.time(9, 0)
RANGE_END = datetime.time(9, 20)   # range_open + range_minutes(20)
FLAT = datetime.time(15, 0)
BUDGET = 299_340.0                 # risk_per_trade_krw
NOTIONAL = 10_000_000.0            # notional_per_position
PROFIT_TARGET_R = 1.0
BREAKEVEN_TRIGGER_R = 0.41
# KEPT ratio-ATR tilt (Gate-1b axis)
R_REF, R_WLO, R_WHI, R_ALPHA = 0.07315764, 0.70269755, 1.44548956, 1.0

# ---- frozen screen pre-register (candidate.json is the machine home; this mirrors it) ----
ALPHA = 0.5                        # screen weight exponent == the arm's flip_value
COLLIN_THRESH = 0.70
# Min build-worthy projected RoR improvement. Anchored to the amihud-materiality precedent
# (docs/solutions/conventions/first-order-materiality-prediction-ignores-notional-ceiling.md):
# a screen ror_shift of +0.0309 there landed −0.0116 live (mis-sign ~0.04), so sub-0.001
# projected shifts are within demonstrated screen-prediction noise and do NOT predict a KEEP —
# the amihud lever cleared the looser 0.00065 floor, built, and REVERTED. 0.005 sits below the
# smallest historically-KEPT lever's realized gain (ratio-ATR +0.0091) yet far above the
# pessimistic-fill downward bias on the decisive reading (bounded ~0.0016 over the ≤6 converted
# trades), so the NO-BUILD is robust even generously fill-corrected.
ROR_SHIFT_FLOOR = 0.005
RES_MIX_FLOOR = 0.05               # min fraction of trades whose resolution class must move (mirrors amihud qty floor)
# Signal ids (documented, stable) — winning_signal_id carries one of these.
SIGNALS = [
    (1, "orwidth_atr"),   # OR-width / prior-ATR ratio
    (2, "minutes"),       # minutes since session open at entry
    (3, "gap"),           # overnight-gap magnitude (today_open/prior_close − 1)
    (4, "orposition"),    # entry location within the opening range: (entry − range_low)/(range_high − range_low)
]


def require(cond, msg):
    if not cond:
        raise RuntimeError(msg)


def dec(b):
    return struct.unpack("<q", b)[0] / SCALE


def kst_dt(ns):
    return datetime.datetime(1970, 1, 1) + datetime.timedelta(microseconds=ns / 1000) + KST


def kst_date(ns):
    return kst_dt(ns).date()


_daily_cache = {}
_minute_cache = {}


def load_daily(sym):
    if sym in _daily_cache:
        return _daily_cache[sym]
    by = {}
    for f in sorted(glob.glob(str(CATALOG / f"{sym}-1-DAY-LAST-EXTERNAL" / "*.parquet"))):
        d = pq.read_table(f).to_pydict()
        for i in range(len(d["ts_event"])):
            by[kst_date(d["ts_event"][i])] = (
                dec(d["open"][i]), dec(d["high"][i]), dec(d["low"][i]),
                dec(d["close"][i]), dec(d["volume"][i]),
            )
    _daily_cache[sym] = by
    return by


def load_minute(sym):
    """Sorted (ts_event, high, low, close) for the symbol."""
    if sym in _minute_cache:
        return _minute_cache[sym]
    rows = []
    for f in sorted(glob.glob(str(CATALOG / f"{sym}-1-MINUTE-LAST-EXTERNAL" / "*.parquet"))):
        d = pq.read_table(f, columns=["high", "low", "close", "ts_event"]).to_pydict()
        for i in range(len(d["ts_event"])):
            rows.append((d["ts_event"][i], dec(d["high"][i]), dec(d["low"][i]), dec(d["close"][i])))
    rows.sort(key=lambda r: r[0])
    _minute_cache[sym] = rows
    return rows


def prior_atr(by, sess, window=ATR_WINDOW):
    """Exact port of backtest.rs::prior_atr — the ratio-ATR cohort's ATR."""
    priors = sorted((dt, v) for dt, v in by.items() if dt < sess)
    if len(priors) < window + 1:
        return None
    tail = priors[-(window + 1):]
    sum_tr = 0.0
    for k in range(1, len(tail)):
        pc = tail[k - 1][1][3]
        _, h, l, _, _ = tail[k][1]
        sum_tr += max(h - l, abs(h - pc), abs(l - pc))
    return sum_tr / window


def prior_close_today_open(by, sess):
    priors = sorted(dt for dt in by if dt < sess)
    if not priors or sess not in by:
        return None, None
    return by[priors[-1]][3], by[sess][0]


def opening_range(sym, sess):
    """(range_high, range_low) as max/min minute high/low over [range_open, range_end)."""
    hi = lo = None
    for ts, h, l, _c in load_minute(sym):
        t = kst_dt(ts)
        if t.date() != sess or not (RANGE_OPEN <= t.time() < RANGE_END):
            continue
        hi = h if hi is None else max(hi, h)
        lo = l if lo is None else min(lo, l)
    return hi, lo


def pearson(xs, ys):
    n = len(xs)
    mx, my = sum(xs) / n, sum(ys) / n
    cov = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    vx = sum((x - mx) ** 2 for x in xs)
    vy = sum((y - my) ** 2 for y in ys)
    if vx == 0 or vy == 0:
        return float("nan")
    return cov / math.sqrt(vx * vy)


def percentile(sorted_v, q):
    """numpy-default linear interpolation (frozen)."""
    n = len(sorted_v)
    if n == 1:
        return sorted_v[0]
    pos = q * (n - 1)
    lo = math.floor(pos)
    frac = pos - lo
    if lo + 1 >= n:
        return sorted_v[lo]
    return sorted_v[lo] + frac * (sorted_v[lo + 1] - sorted_v[lo])


def clamp_weight(signal, ref, w_lo, w_hi):
    """clamp((ref/signal)^alpha, w_lo, w_hi); fail-closed neutral on missing/degenerate input.
    `ref <= 0.0` is guarded too: a non-positive base under the fractional `alpha` would be a
    Python complex number and crash `min/max` (reachable if a future signal — e.g. a negative
    overnight gap — makes the median `ref` non-positive; every v32 signal here is strictly
    positive, so this is fail-closed defence for reuse, not a live path)."""
    if signal is None or signal <= 0.0 or ref <= 0.0:
        return 1.0
    return min(max((ref / signal) ** ALPHA, w_lo), w_hi)


def ratio_atr_weight(atr, price):
    if atr is None or atr <= 0.0 or price <= 0.0:
        return 1.0
    v = atr / price
    return min(max((R_REF / v) ** R_ALPHA, R_WLO), R_WHI)


def qty_at(weight, rps_scaled, w_ratio, price):
    """CLASS-B qty: min(floor(budget·w_ratio / rps_scaled), floor(notional/price))."""
    return min(math.floor(BUDGET * w_ratio / rps_scaled), math.floor(NOTIONAL / price))


def simulate(sym, sess, entry, range_low, range_high, entry_ts, stop_dist):
    """Return (resolution, exit_price) for a RangeLow trade whose initial stop distance is
    `stop_dist` (KRW). Target/breakeven are DECOUPLED from the stop (r_denom = OR-width, fixed).
    Close-confirm entry → walk bars strictly AFTER the entry bar (no same-bar stop). Stop-first
    pessimism; stop/timeflat fill at the bar low, target at the target price; breakeven ratchet
    to entry once high-water reaches the trigger, binding from the next bar."""
    entry = round(entry)
    r_denom = round(range_high) - round(range_low)   # OR-width, fixed (decoupled)
    stop = entry - max(1, round(stop_dist))
    target = entry + round(PROFIT_TARGET_R * r_denom) if r_denom > 0 else None
    be_trigger = entry + round(BREAKEVEN_TRIGGER_R * r_denom) if r_denom > 0 else None
    high_water = entry
    bars = [b for b in load_minute(sym) if kst_dt(b[0]).date() == sess and b[0] > entry_ts]
    for ts, high, low, close in bars:
        if kst_dt(ts).time() >= FLAT:
            return "timeflat", round(low)          # time-flat exit at the flat bar's low
        low_i, high_i = round(low), round(high)
        if low_i <= stop:                          # stop-first
            return "stop", low_i
        if target is not None and high_i >= target:
            return "target", target
        high_water = max(high_water, high_i)
        if be_trigger is not None and high_water >= be_trigger:
            stop = max(stop, entry)                # breakeven ratchet (trail off), binds next bar
    if bars:
        return "timeflat", round(bars[-1][3])      # no flat bar in the tail — book the last close
    return "timeflat", entry


def build_rows():
    require(len(sys.argv) >= 2, "missing readings output path")
    manifest = json.load(open(RUN_HOME / "manifest.json", encoding="utf-8"))
    # R6 + identity: assert the frozen head and its RangeLow stop mode before any reading.
    for name, value in [
        ("run_id", RUN_ID), ("strategy_version", STRATEGY_VERSION),
        ("strategy_code_hash", STRATEGY_HASH), ("catalog_fingerprint", CATALOG_FINGERPRINT),
        ("universe_hash", UNIVERSE_HASH),
    ]:
        require(manifest.get(name) == value, f"head identity mismatch: {name}")
    require(manifest.get("data_range") == {"start": DATA_START, "end": DATA_END}, "data-range mismatch")
    params = manifest.get("params", {})
    require(params.get("stop_mode", 0.0) == 0.0, "head is not RangeLow (stop_mode != 0) — R6 premise broken")
    require(params.get("range_minutes") == 20 and params.get("range_open") == "09:00:00", "opening-range window drift")

    perf = json.load(open(RUN_HOME / "performance.json", encoding="utf-8"))
    closed = [t for t in perf["trades"] if t.get("ts_closed") is not None]

    rows = []
    for t in closed:
        q = t["quantity"]
        rc = t.get("risk_capital")
        if rc is None or q <= 0:
            continue
        sym = t["symbol"]
        sess = kst_date(t["ts_opened"])
        entry = t["avg_px_open"]
        rps = rc / q                                 # entry − stop (RangeLow)
        by = load_daily(sym)
        atr = prior_atr(by, sess)
        prior_close, today_open = prior_close_today_open(by, sess)
        rh, rl = opening_range(sym, sess)
        buy = next(f for f in t["fills"] if f["side"] == "BUY")
        entry_ts = buy["ts_event"]
        et = kst_dt(entry_ts)
        minutes = (et.hour * 60 + et.minute + et.second / 60.0) - 540.0
        r_denom = (rh - rl) if (rh is not None and rl is not None) else None
        rows.append({
            "sym": sym, "sess": sess, "entry": entry, "rps": rps, "rc": rc, "q": q,
            "entry_ts": entry_ts, "rl": rl, "rh": rh, "r_denom": r_denom,
            "w_ratio": ratio_atr_weight(atr, entry),
            "orwidth_atr": (r_denom / atr) if (r_denom and atr and atr > 0) else None,
            "minutes": minutes,
            "gap": ((today_open / prior_close - 1.0) if (prior_close and prior_close > 0) else None),
            "orposition": ((entry - rl) / r_denom) if (r_denom and r_denom > 0) else None,
        })
    require(rows, "no closed trades with risk capital")
    return rows


def signal_band(rows, key):
    coh = [r[key] for r in rows if r[key] is not None]
    sv = sorted(coh)
    ref = percentile(sv, 0.5)
    p10 = percentile(sv, 0.10)
    p90 = percentile(sv, 0.90)
    # Non-positive ref/percentiles (a mixed- or negative-sign signal distribution) would make
    # the fractional-power band complex → crash; fail closed to a neutral (all-weight-1.0) band.
    if ref <= 0.0 or p10 <= 0.0 or p90 <= 0.0:
        return ref, 1.0, 1.0
    w_lo = (ref / p90) ** ALPHA
    w_hi = (ref / p10) ** ALPHA
    return ref, w_lo, w_hi


def baseline_sim(rows):
    """sim(w=1): per-trade resolution + (pnl, risk_capital) baseline for the shift."""
    pnl = 0.0
    rc = 0.0
    for r in rows:
        q1 = qty_at(1.0, r["rps"], r["w_ratio"], r["entry"])
        reso, ex = simulate(r["sym"], r["sess"], r["entry"], r["rl"], r["rh"], r["entry_ts"], r["rps"])
        r["res1"] = reso
        pnl += q1 * (ex - round(r["entry"]))
        rc += q1 * max(1.0, r["rps"])
    return pnl / rc, pnl, rc


def evaluate_signal(rows, key, ror_baseline):
    """Return the four canonical readings + band for one signal."""
    ref, w_lo, w_hi = signal_band(rows, key)
    # Gate 1 cohort = trades whose signal is available.
    coh = [r for r in rows if r[key] is not None]
    ws = [clamp_weight(r[key], ref, w_lo, w_hi) for r in coh]
    collin_rps = abs(pearson(ws, [r["rps"] for r in coh]))
    collin_ratio = abs(pearson(ws, [r["w_ratio"] for r in coh]))
    # Gate 2 materiality over ALL rows (w=1 where signal absent — skip-not-reject).
    pnl = rc = 0.0
    changed = 0
    for r in rows:
        w = clamp_weight(r[key], ref, w_lo, w_hi) if r[key] is not None else 1.0
        rps_scaled = max(1.0, w * r["rps"])
        qw = qty_at(w, rps_scaled, r["w_ratio"], r["entry"])
        reso, ex = simulate(r["sym"], r["sess"], r["entry"], r["rl"], r["rh"], r["entry_ts"], w * r["rps"])
        pnl += qw * (ex - round(r["entry"]))
        rc += qw * rps_scaled
        if reso != r["res1"]:
            changed += 1
    ror_shift = (pnl / rc) - ror_baseline
    res_mix_shift = changed / len(rows)
    return {
        "collin_abs_rps": collin_rps,
        "collin_abs_ratio_atr": collin_ratio,
        "ror_shift": ror_shift,
        "resolution_mix_shift": res_mix_shift,
        "ref": ref, "w_lo": w_lo, "w_hi": w_hi,
    }


def clears(rd):
    return (rd["collin_abs_rps"] < COLLIN_THRESH and rd["collin_abs_ratio_atr"] < COLLIN_THRESH
            and rd["ror_shift"] >= ROR_SHIFT_FLOOR and rd["resolution_mix_shift"] >= RES_MIX_FLOOR)


def main():
    out_path = sys.argv[-1]
    rows = build_rows()
    ror_baseline, _, _ = baseline_sim(rows)

    evals = {}
    for sid, key in SIGNALS:
        evals[sid] = evaluate_signal(rows, key, ror_baseline)

    # Winner selection INSIDE the diagnostic (KTD7): among signals clearing all four gates,
    # the largest projected RoR-shift; if NONE clears, the best-by-ror_shift among all four
    # (deterministic tie-break on signal id) — its readings then fail a threshold → STOP.
    clearing = [sid for sid, _ in SIGNALS if clears(evals[sid])]
    pool = clearing if clearing else [sid for sid, _ in SIGNALS]
    winner = max(pool, key=lambda sid: (evals[sid]["ror_shift"], -sid))
    wd = evals[winner]

    # ---- human-readable report (stdout; the gate reads only the JSON file) ----
    print(f"v32 head RangeLow screen — closed trades: {len(rows)}   sim(w=1) RoR baseline {ror_baseline:.4f}")
    for sid, key in SIGNALS:
        d = evals[sid]
        g1a = "GO" if d["collin_abs_rps"] < COLLIN_THRESH else "STOP"
        g1b = "GO" if d["collin_abs_ratio_atr"] < COLLIN_THRESH else "STOP"
        g2a = "GO" if d["ror_shift"] >= ROR_SHIFT_FLOOR else "STOP"
        g2b = "GO" if d["resolution_mix_shift"] >= RES_MIX_FLOOR else "STOP"
        print(f"  [{sid}] {key:11} |r_rps|={d['collin_abs_rps']:.4f}({g1a}) |r_ratio|={d['collin_abs_ratio_atr']:.4f}({g1b}) "
              f"ror_shift={d['ror_shift']:+.6f}({g2a}) res_mix={d['resolution_mix_shift']:.4f}({g2b})")
    print(f"clearing signals: {clearing or 'NONE'} -> winner id {winner} "
          f"({'BUILD' if clearing else 'best-failing → NO-BUILD'})")

    # ---- canonical readings artifact (the gate reads THIS) ----
    readings = {
        "collin_abs_rps": round(wd["collin_abs_rps"], 4),
        "collin_abs_ratio_atr": round(wd["collin_abs_ratio_atr"], 4),
        "ror_shift": round(wd["ror_shift"], 6),
        "resolution_mix_shift": round(wd["resolution_mix_shift"], 4),
        "winning_signal_id": float(winner),
        # winner companion seeds — extra numeric keys that flow into diagnostic_readings (U5/U6).
        "stop_width_ref": round(wd["ref"], 8),
        "stop_width_w_lo": round(wd["w_lo"], 8),
        "stop_width_w_hi": round(wd["w_hi"], 8),
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(readings, fh, sort_keys=True)


if __name__ == "__main__":
    main()
