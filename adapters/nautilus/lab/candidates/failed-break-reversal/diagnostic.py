#!/usr/bin/env python3
"""Phase-A dual-grammar screen for the failed-break REVERSAL entry stream on v32 (plan 2026-07-22-001).

Lever 8 adds a second, long-only entry stream to the ORB strategy that trades the FAILURE of a
confirmed downside break of the opening range — a confirmed close below the fixed range low
followed by a confirmed close back above it before flat time. The v32 breakout leg is untouched;
this is an ADDITIVE stream, so its trades do not exist in the head run and there is no incumbent
signal to correlate against (the stop-geometry collinearity gates are therefore dropped — KTD3).

The screen measures TWO candidate grammars from ONE bar sweep and lets the gate pick the surviving
one or NO-BUILD (KTD4):

  * Grammar A (PRIMARY, the true inversion) — breakdown-recovery. Over every selected symbol-session
    that took NO v32 trade (the pure additive population — "sessions that currently never enter";
    displacement of v32 breakouts is a substitution measured only at the flip, never here), detect a
    confirmed breakdown (a bar CLOSE strictly below the range low, in `[range_end, flat)`) followed by
    a confirmed recovery (a later bar CLOSE strictly above the range low, still inside the range). The
    reversal long enters at the recovery close. A stop-anchor sweep (breakdown session low vs range
    low) picks the best-by-ror_shift anchor (KTD5 — the anchor only moves the stop, so like the
    breakout leg it is CLASS-B-absorbed on the RoR denominator; only stop-out geometry survives).

  * Grammar B (SECONDARY, capped by v32's own trade count) — post-stop re-entry. Over every v32 Long
    that resolves to `stop` under this screen's own barrier re-sim, find a later close back above the
    range low (before flat) and re-enter long at that close, anchored at the range low. A grammar-B
    win RETURNS TO PLANNING (it makes the session-terminal state re-entrant — a structurally different
    change the build units do not cover), so this grammar is computed but never built from here.

GATES (frozen in candidate.json; KTD3). An additive stream keeps two STOP gates:

  Gate 1 (population-count floor):  population_count >= COUNT_FLOOR   — a thin population is a NO-BUILD
                                                                        regardless of projected shift.
  Gate 2 (RoR materiality):         ror_shift        >= ROR_SHIFT_FLOOR

`resolution_target_share` (and the full stop/target/timeflat mix) is the FILL-PRICE-INDEPENDENT
PRIMARY reading — which barrier a trade hits first is pure geometry, independent of the fill price,
whereas any qty-weighted P&L stat inherits the pessimistic-flat-fill bias — but it is a RECORDED
reading, not a hard threshold: the two pre-registered STOP gates are the count floor and ror_shift
(Success Criteria). It is emitted, twin-agreed, and disclosed so a reviewer reads the population's
quality alongside the gated numbers.

`ror_shift` is the ADDITIVE shift `RoR(base + winner) - RoR(base)`, both under this screen's own
barrier re-sim of the v32 baseline (NOT the run's realized 0.1876), so the pessimistic-flat-fill
bias cancels to first order (the stop-geometry / amihud precedent). Sizing is CLASS-B and
CEILING-AWARE — `min(floor(budget*w_ratio/rps), floor(notional/price))` — the notional clip that the
amihud mis-prediction omitted. `winning_grammar_id` (tolerance 0) forces the independently authored
twin to agree on the argmax; `stop_anchor_id` rides along as the winner's companion seed.

Structural independence (KTD4): this script reconstructs ENTRY-LOCALLY (each symbol-session loads its
own bars on demand, gap-retention style); the twin builds CATALOG-WIDE maps. Both share the barrier
SEMANTICS by design (RangeLow decoupled target/breakeven, close-confirm entry so no same-bar stop,
stop-first pessimism, breakeven ratchet to entry) — the independence is in reconstruction and
statistics, the parts a shared bug could corrupt.
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
BARS_HOME = DATA_HOME / "catalog" / "data" / "bars"

# ---- frozen v32 params (manifest, not source defaults) ----
SCALE = 1_000_000_000
KST = dt.timezone(dt.timedelta(hours=9))
ATR_WINDOW = 14
RANGE_OPEN = dt.time(9, 0)
RANGE_END = dt.time(9, 20)          # range_open + range_minutes(20)
FLAT = dt.time(15, 0)
BUDGET = 299_340.0                  # risk_per_trade_krw
NOTIONAL = 10_000_000.0             # notional_per_position
PROFIT_TARGET_R = 1.0
BREAKEVEN_TRIGGER_R = 0.41
GAP_MIN_PCT = 0.6                   # universe gap filter (percent)
OR_WIDTH_MAX_ATR = 0.666           # OR-width session gate
GAP_RETENTION_MIN = 0.5            # gap-retention session gate
# KEPT ratio-ATR tilt (R8 — reversal inherits it unchanged)
R_REF, R_WLO, R_WHI, R_ALPHA = 0.07315764, 0.70269755, 1.44548956, 1.0

# ---- frozen screen pre-register (candidate.json is the machine home; this mirrors it) ----
# Min build-worthy ADDITIVE projected RoR improvement. Same standing floor as the stop-geometry
# precedent: 0.005 sits below the smallest historically-KEPT lever gain (ratio-ATR +0.0091) yet far
# enough above screen-prediction noise (the amihud lever cleared a 0.00065 floor, built, and
# REVERTED) that a NO-BUILD is robust even generously fill-corrected.
ROR_SHIFT_FLOOR = 0.005
# Min build-worthy population. Below this the additive RoR estimate is dominated by a handful of
# trades AND — under the shared `max_concurrent 7` budget (KTD3 caveat) — the realized post-contention
# population would be thinner still, so a sub-floor count is a NO-BUILD on its own regardless of the
# projected shift. Set to twice the stop-geometry resolution-moved scale (~6 trades over 77).
COUNT_FLOOR = 12
# Grammar ids (documented, stable) — winning_grammar_id carries one of these.
GRAMMAR_A = 1.0   # breakdown-recovery (primary — the true inversion mechanism)
GRAMMAR_B = 2.0   # post-stop re-entry (secondary — a grammar-B win returns to planning)
# Stop-anchor ids (documented, stable) — stop_anchor_id carries the winner's anchor.
ANCHOR_BREAKDOWN_LOW = 1.0   # lowest low from the breakdown bar through the recovery (entry) bar
ANCHOR_RANGE_LOW = 2.0       # the opening-range low that was broken and recovered above


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
    """Re-derive the catalog content fingerprint over the frozen range (identity, U1)."""
    start_ns = int(dt.datetime(2026, 5, 26, tzinfo=KST).timestamp()) * SCALE
    end_ns = int(dt.datetime(2026, 7, 4, tzinfo=KST).timestamp()) * SCALE - 1
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


# ---- entry-local bar loaders (each symbol-session loads its own bars on demand) ----

def in_window(session):
    return dt.date(2026, 5, 26) <= session <= dt.date(2026, 7, 3)


def daily_map(symbol):
    """{session_date: (open, high, low, close)} in integer KRW, latest bar per date wins."""
    pattern = str(BARS_HOME / f"{symbol}-1-DAY-LAST-EXTERNAL" / "*.parquet")
    files = glob.glob(pattern)
    require(files, f"missing daily bars for {symbol}")
    latest_ts = {}
    out = {}
    for file_name in sorted(files):
        table = pq.read_table(file_name, columns=["open", "high", "low", "close", "ts_event"]).to_pydict()
        for i in range(len(table["ts_event"])):
            ts = table["ts_event"][i]
            day = kst_stamp(ts).date()
            if day not in latest_ts or ts > latest_ts[day]:
                latest_ts[day] = ts
                out[day] = (
                    canonical_price(table["open"][i]), canonical_price(table["high"][i]),
                    canonical_price(table["low"][i]), canonical_price(table["close"][i]),
                )
    return out


def minute_sessions(symbol):
    """{session_date: sorted [(ts, high, low, close)]} for the symbol, in integer KRW.

    A divergent duplicate minute bar (two rows at the same `ts_event` disagreeing on OHLC) ABORTS
    — the gap-retention precedent's guard. This is the one whole-universe scan input, so a corrupted
    or duplicated parquet row would otherwise be silently folded into the barrier walk; because the
    same omission would live in the twin too, twin agreement could not catch it. An exact duplicate
    is de-duplicated (kept once) so it is never walked twice.
    """
    pattern = str(BARS_HOME / f"{symbol}-1-MINUTE-LAST-EXTERNAL" / "*.parquet")
    files = glob.glob(pattern)
    require(files, f"missing minute bars for {symbol}")
    sessions = {}
    seen = {}
    for file_name in sorted(files):
        table = pq.read_table(file_name, columns=["high", "low", "close", "ts_event"]).to_pydict()
        for i in range(len(table["ts_event"])):
            ts = table["ts_event"][i]
            bar = (canonical_price(table["high"][i]), canonical_price(table["low"][i]), canonical_price(table["close"][i]))
            if ts in seen:
                require(seen[ts] == bar, f"divergent duplicate minute bar for {symbol} at ts {ts}")
                continue
            seen[ts] = bar
            sessions.setdefault(kst_stamp(ts).date(), []).append((ts, *bar))
    for day in sessions:
        sessions[day].sort(key=lambda r: r[0])
    return sessions


def prior_atr(dmap, session, window=ATR_WINDOW):
    """Exact port of backtest.rs::prior_atr — the ratio-ATR / OR-width cohort's ATR."""
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


def opening_range(bars, session):
    """(range_high, range_low) as max/min minute high/low over [range_open, range_end)."""
    hi = lo = None
    for ts, h, l, _c in bars:
        t = kst_stamp(ts).time()
        if not (RANGE_OPEN <= t < RANGE_END):
            continue
        hi = h if hi is None else max(hi, h)
        lo = l if lo is None else min(lo, l)
    return hi, lo


def ratio_atr_weight(atr, price):
    """The KEPT ratio-ATR tilt weight (R8): clamp((ref/(atr/price))^alpha, w_lo, w_hi)."""
    if atr is None or atr <= 0.0 or price <= 0.0:
        return 1.0
    v = atr / price
    return min(max((R_REF / v) ** R_ALPHA, R_WLO), R_WHI)


def qty_at(w_ratio, rps, price):
    """CLASS-B ceiling-aware qty: min(floor(budget*w_ratio / rps), floor(notional/price))."""
    if rps <= 0.0 or price <= 0.0:
        return 0
    return min(math.floor(BUDGET * w_ratio / rps), math.floor(NOTIONAL / price))


def simulate(bars, entry, range_high, range_low, entry_ts, stop):
    """(resolution, exit_price) for a long entered at `entry` (integer KRW) with initial `stop`.

    RangeLow-decoupled geometry: target/breakeven use r_denom = range_high - range_low, FIXED
    (unchanged by the stop). Close-confirm entry → walk bars strictly AFTER the entry bar (no
    same-bar stop). Stop-first pessimism; stop/timeflat fill at the bar low, target at the target
    price; breakeven ratchet to entry once high-water reaches the trigger, binding from the next bar.
    """
    r_denom = range_high - range_low
    target = entry + round(PROFIT_TARGET_R * r_denom) if r_denom > 0 else None
    be_trigger = entry + round(BREAKEVEN_TRIGGER_R * r_denom) if r_denom > 0 else None
    high_water = entry
    seq = [b for b in bars if b[0] > entry_ts]
    last_close = None
    for ts, high, low, close in seq:
        last_close = close
        if kst_stamp(ts).time() >= FLAT:
            return "timeflat", low
        if low <= stop:                       # stop-first pessimism
            return "stop", low
        if target is not None and high >= target:
            return "target", target
        high_water = max(high_water, high)
        if be_trigger is not None and high_water >= be_trigger:
            stop = max(stop, entry)           # breakeven ratchet (trail off), binds next bar
    if last_close is not None:
        return "timeflat", last_close         # no flat bar in the tail — book the last close
    return "timeflat", entry


# ---------------------------------------------------------------------------
# v32 baseline re-sim (the ADDITIVE shift's common denominator)
# ---------------------------------------------------------------------------

def build_baseline(perf):
    """Re-sim v32's closed breakout trades under this screen's barrier model.

    Returns (pnl, rc, trades). Sizing is reconstructed CLASS-B and asserted EXACTLY against each
    trade's recorded quantity — a mismatch means the ATR / tilt / sizing reconstruction is wrong and
    the screen must fail at cause, never silently. Each baseline trade also carries its resolution,
    range bounds, and reconstruction inputs (grammar B re-enters from the `stop`-resolved ones).
    """
    closed = [t for t in perf["trades"] if t.get("ts_closed") is not None]
    pnl = rc = 0.0
    trades = []
    dmap_cache = {}
    mins_cache = {}
    for t in closed:
        q = t["quantity"]
        rc_trade = t.get("risk_capital")
        if rc_trade is None or q <= 0:
            continue
        sym = t["symbol"]
        sess = kst_stamp(t["ts_opened"]).date()
        entry = round(t["avg_px_open"])
        rps = rc_trade / q                     # entry - stop (RangeLow), the recorded per-share risk
        if sym not in dmap_cache:
            dmap_cache[sym] = daily_map(sym)
            mins_cache[sym] = minute_sessions(sym)
        atr = prior_atr(dmap_cache[sym], sess)
        w_ratio = ratio_atr_weight(atr, entry)
        rh, rl = opening_range(mins_cache[sym].get(sess, []), sess)
        require(rh is not None and rl is not None, f"missing opening range for baseline {sym} {sess}")
        # Reconstruct qty EXACTLY (identity check on the ATR/tilt/sizing model).
        q_recon = qty_at(w_ratio, rps, entry)
        require(q_recon == int(q), f"baseline qty reconstruction mismatch {sym} {sess}: {q_recon} vs {int(q)}")
        buy = next(f for f in t["fills"] if f["side"] == "BUY")
        entry_ts = buy["ts_event"]
        reso, ex = simulate(mins_cache[sym].get(sess, []), entry, rh, rl, entry_ts, entry - round(rps))
        pnl += q_recon * (ex - entry)
        rc += q_recon * max(1.0, rps)
        trades.append({
            "sym": sym, "sess": sess, "entry": entry, "rps": rps, "w_ratio": w_ratio,
            "rh": rh, "rl": rl, "ts_closed": t["ts_closed"], "resolution": reso,
        })
    require(trades, "no closed baseline trades with risk capital")
    return pnl, rc, trades


# ---------------------------------------------------------------------------
# Grammar A — breakdown-recovery (primary)
# ---------------------------------------------------------------------------

def find_breakdown_recovery(bars, range_high, range_low):
    """Detect a confirmed breakdown followed by a confirmed recovery in `[range_end, flat)`.

    Returns (recovery_ts, recovery_close, breakdown_low) or None. Breakdown confirms on a bar CLOSE
    strictly below the range low (a wick-below-close-inside does NOT confirm — AE2). Recovery
    confirms on a LATER bar CLOSE strictly above the range low and inside the range (a close above
    the range high would be the breakout leg — it wins per KTD6 — and such a session took a v32 trade
    and is already excluded). `breakdown_low` is the lowest low from the breakdown bar through the
    recovery bar inclusive.
    """
    breakdown_low = None
    for ts, high, low, close in bars:
        t = kst_stamp(ts).time()
        if t < RANGE_END:
            continue
        if t >= FLAT:
            break
        if breakdown_low is None:
            if close < range_low:                     # confirmed breakdown
                breakdown_low = low
            continue
        breakdown_low = min(breakdown_low, low)
        if close > range_high:                        # breakout leg wins (KTD6) — not a reversal
            return None
        if close > range_low:                         # confirmed recovery back into the range
            return ts, close, min(breakdown_low, low)
    return None


def grammar_a_trades(perf, traded_sessions):
    """Hypothetical reversal longs over the pure additive population (KTD5).

    One reversal candidate per selected symbol-session that (a) took NO v32 trade (mutual
    exclusivity — the additive population), (b) passes the same session gates as a breakout entry
    (gap applicability, OR-width, gap-retention — R8), and (c) exhibits a confirmed
    breakdown-recovery. Each candidate is scored under BOTH stop anchors; the caller picks the
    best-by-ror_shift anchor across the whole population.
    """
    symbols = sorted({p.name.split("-1-MINUTE-")[0] for p in BARS_HOME.glob("*-1-MINUTE-LAST-EXTERNAL")})
    candidates = []
    for sym in symbols:
        dmap = daily_map(sym)
        sessions = minute_sessions(sym)
        for sess, bars in sessions.items():
            if not in_window(sess):
                continue
            if (sym, sess) in traded_sessions:        # mutual exclusivity — additive population only
                continue
            # Gap applicability (universe gap filter) + gap-retention preconditions need the daily gap.
            if sess not in dmap:
                continue
            today_open = dmap[sess][0]
            priors = sorted(d for d in dmap if d < sess)
            if not priors:
                continue
            prior_close = dmap[priors[-1]][3]
            if prior_close <= 0:
                continue
            gap_pct = (today_open - prior_close) / prior_close * 100.0
            if gap_pct < GAP_MIN_PCT:                 # gap filter (universe membership)
                continue
            rh, rl = opening_range(bars, sess)
            if rh is None or rl is None or rl <= 0:
                continue
            atr = prior_atr(dmap, sess)
            # OR-width gate (skip-not-reject when no positive prior ATR — the orb.rs semantics).
            if OR_WIDTH_MAX_ATR > 0.0 and atr is not None and atr > 0.0:
                if (rh - rl) > OR_WIDTH_MAX_ATR * atr:
                    continue
            # Gap-retention gate: retention = (range_low - prior_close)/(today_open - prior_close).
            if today_open <= prior_close:             # not-applicable (non-positive gap) — no arm
                continue
            retention = (rl - prior_close) / (today_open - prior_close)
            if not math.isfinite(retention) or retention > 1.0 or retention < GAP_RETENTION_MIN:
                continue
            found = find_breakdown_recovery(bars, rh, rl)
            if found is None:
                continue
            rec_ts, entry, breakdown_low = found
            if rh - rl <= 0:                          # degenerate range — no target geometry
                continue
            w_ratio = ratio_atr_weight(atr, entry)
            candidates.append({
                "sym": sym, "sess": sess, "entry": entry, "rh": rh, "rl": rl,
                "breakdown_low": breakdown_low, "rec_ts": rec_ts, "w_ratio": w_ratio, "bars": bars,
            })
    return candidates


def score_population(candidates, anchor_of, base_pnl, base_rc, base_ror):
    """Score a hypothetical reversal population under a per-candidate stop anchor.

    Returns (population_count, ror_shift, target_share, stop_share, timeflat_share). A candidate whose
    recovery close equals its stop anchor (zero stop distance) is REJECTED at sizing (KTD6 — the
    ATR-zero lesson class), never divided through. `ror_shift` is the additive
    RoR(base + population) - RoR(base).
    """
    pnl = rc = 0.0
    n = 0
    reso_counts = {"stop": 0, "target": 0, "timeflat": 0}
    for c in candidates:
        stop = anchor_of(c)
        rps = c["entry"] - stop
        if rps <= 0:                                  # degenerate stop distance — reject at sizing
            continue
        qty = qty_at(c["w_ratio"], rps, c["entry"])
        if qty <= 0:
            continue
        reso, ex = simulate(c["bars"], c["entry"], c["rh"], c["rl"], c["rec_ts"], stop)
        pnl += qty * (ex - c["entry"])
        rc += qty * max(1.0, rps)
        reso_counts[reso] += 1
        n += 1
    if n == 0:
        return 0, 0.0, 0.0, 0.0, 0.0
    ror_shift = (base_pnl + pnl) / (base_rc + rc) - base_ror
    return (
        n, ror_shift,
        reso_counts["target"] / n, reso_counts["stop"] / n, reso_counts["timeflat"] / n,
    )


# ---------------------------------------------------------------------------
# Grammar B — post-stop re-entry (secondary)
# ---------------------------------------------------------------------------

def grammar_b_candidates(perf, baseline_trades):
    """Post-stop re-entry candidates: each v32 Long that resolves to `stop`, re-entered at the first
    later close back above the range low (before flat), anchored at the range low (KTD5)."""
    mins_cache = {}
    dmap_cache = {}
    candidates = []
    for bt in baseline_trades:
        if bt["resolution"] != "stop":
            continue
        sym, sess = bt["sym"], bt["sess"]
        if sym not in mins_cache:
            mins_cache[sym] = minute_sessions(sym)
            dmap_cache[sym] = daily_map(sym)
        bars = mins_cache[sym].get(sess, [])
        rh, rl = bt["rh"], bt["rl"]
        # Re-entry: first bar AFTER the stop close with a close back above the range low, before flat.
        rec = None
        for ts, high, low, close in bars:
            if ts <= bt["ts_closed"]:
                continue
            if kst_stamp(ts).time() >= FLAT:
                break
            if close > rh:                            # a breakout close is the breakout leg, not this
                rec = None
                break
            if close > rl:
                rec = (ts, close)
                break
        if rec is None:
            continue
        rec_ts, entry = rec
        if rh - rl <= 0:
            continue
        atr = prior_atr(dmap_cache[sym], sess)
        w_ratio = ratio_atr_weight(atr, entry)
        candidates.append({
            "sym": sym, "sess": sess, "entry": entry, "rh": rh, "rl": rl,
            "rec_ts": rec_ts, "w_ratio": w_ratio, "bars": bars,
        })
    return candidates


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

def assert_identity(manifest):
    expected = {
        "run_id": RUN_ID, "strategy_id": "orb", "strategy_version": STRATEGY_VERSION,
        "strategy_code_hash": STRATEGY_HASH, "catalog_fingerprint": CATALOG_FINGERPRINT,
        "universe_hash": UNIVERSE_HASH,
    }
    for name, value in expected.items():
        require(manifest.get(name) == value, f"head identity mismatch: {name}")
    require(actual_catalog_fingerprint() == CATALOG_FINGERPRINT, "catalog content fingerprint mismatch")
    require(manifest.get("data_range") == {"start": DATA_START, "end": DATA_END}, "data-range mismatch")
    params = manifest.get("params", {})
    require(params.get("stop_mode", 0.0) == 0.0, "head is not RangeLow (stop_mode != 0) — R6 premise broken")
    require(params.get("entry_confirm") == 1.0, "head is not close-confirm (entry_confirm != 1) — KTD6 premise broken")
    require(params.get("range_minutes") == 20 and params.get("range_open") == "09:00:00", "opening-range window drift")


def main():
    require(len(sys.argv) >= 2, "missing readings output path")
    out_path = sys.argv[-1]
    manifest = json.load(open(RUN_HOME / "manifest.json", encoding="utf-8"))
    assert_identity(manifest)
    perf = json.load(open(RUN_HOME / "performance.json", encoding="utf-8"))

    base_pnl, base_rc, baseline = build_baseline(perf)
    require(base_rc > 0, "undefined baseline RoR denominator")
    base_ror = base_pnl / base_rc

    traded_sessions = {(t["symbol"], kst_stamp(t["ts_opened"]).date()) for t in perf["trades"]}

    # ---- Grammar A: breakdown-recovery, best-by-ror_shift stop anchor ----
    a_cands = grammar_a_trades(perf, traded_sessions)
    anchor_fns = {
        ANCHOR_BREAKDOWN_LOW: lambda c: c["breakdown_low"],
        ANCHOR_RANGE_LOW: lambda c: c["rl"],
    }
    a_by_anchor = {}
    for aid, fn in anchor_fns.items():
        a_by_anchor[aid] = score_population(a_cands, fn, base_pnl, base_rc, base_ror)
    # Best anchor by ror_shift (deterministic tie-break: lower anchor id).
    best_anchor = max(anchor_fns, key=lambda aid: (a_by_anchor[aid][1], -aid))
    a_n, a_ror, a_tgt, a_stop, a_flat = a_by_anchor[best_anchor]

    # ---- Grammar B: post-stop re-entry, range-low anchor ----
    b_cands = grammar_b_candidates(perf, baseline)
    b_n, b_ror, b_tgt, b_stop, b_flat = score_population(
        b_cands, lambda c: c["rl"], base_pnl, base_rc, base_ror
    )

    grammars = {
        GRAMMAR_A: {"n": a_n, "ror": a_ror, "tgt": a_tgt, "stop": a_stop, "flat": a_flat, "anchor": best_anchor},
        GRAMMAR_B: {"n": b_n, "ror": b_ror, "tgt": b_tgt, "stop": b_stop, "flat": b_flat, "anchor": ANCHOR_RANGE_LOW},
    }

    def clears(g):
        return g["n"] >= COUNT_FLOOR and g["ror"] >= ROR_SHIFT_FLOOR

    # Winner selection (KTD4): prefer the PRIMARY grammar A when it clears; else grammar B when it
    # clears (→ returns to planning); else the best-by-ror_shift among both (its readings then fail a
    # threshold → the tool records STOP = NO-BUILD).
    if clears(grammars[GRAMMAR_A]):
        winner = GRAMMAR_A
    elif clears(grammars[GRAMMAR_B]):
        winner = GRAMMAR_B
    else:
        winner = max(grammars, key=lambda gid: (grammars[gid]["ror"], -gid))
    w = grammars[winner]

    # ---- human-readable report (stdout; the gate reads only the JSON file) ----
    print(f"v32 baseline: {len(baseline)} breakout trades, sim RoR {base_ror:.4f}")
    for gid, label in [(GRAMMAR_A, "breakdown-recovery"), (GRAMMAR_B, "post-stop re-entry")]:
        g = grammars[gid]
        gc = "GO" if g["n"] >= COUNT_FLOOR else "STOP"
        gr = "GO" if g["ror"] >= ROR_SHIFT_FLOOR else "STOP"
        print(f"  [{int(gid)}] {label:20} n={g['n']:3d}({gc}) ror_shift={g['ror']:+.6f}({gr}) "
              f"target={g['tgt']:.3f} stop={g['stop']:.3f} flat={g['flat']:.3f} anchor={int(g['anchor'])}")
    print(f"winner grammar {int(winner)} anchor {int(w['anchor'])} "
          f"({'BUILD' if clears(grammars[GRAMMAR_A]) else ('RETURN-TO-PLANNING' if clears(grammars[GRAMMAR_B]) else 'NO-BUILD')})")

    # ---- canonical readings artifact (the gate reads THIS) ----
    readings = {
        "population_count": float(w["n"]),
        "ror_shift": round(w["ror"], 6),
        "resolution_target_share": round(w["tgt"], 4),
        "resolution_stop_share": round(w["stop"], 4),
        "resolution_timeflat_share": round(w["flat"], 4),
        "winning_grammar_id": float(winner),
        "stop_anchor_id": float(w["anchor"]),
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(readings, fh, sort_keys=True)


if __name__ == "__main__":
    main()
