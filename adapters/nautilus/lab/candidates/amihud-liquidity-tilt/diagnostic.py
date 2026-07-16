#!/usr/bin/env python3
"""Phase-A dual gate for the Amihud-illiquidity budget tilt on v30 (plan 2026-07-16-003).

A NEW dimensionless sizing axis, orthogonal (hypothesis) to the two the kept levers
already size on: stop distance (`risk_per_share`) and relative volatility
(`prior_atr/entry_price`, the kept ratio-ATR tilt). The lever multiplies the per-trade
risk BUDGET by a dimensionless inverse-illiquidity weight

    illiq = mean over prior 14 sessions of |ret_k| / turnover_k     (turnover_k = close_k*volume_k, KRW)
    w     = clamp( (illiq_ref / illiq)^alpha, w_lo, w_hi )          alpha = 1.0 (flip value)
    qty   = min( floor(risk_per_trade_krw*w / rps), floor(notional / px) )

leaving the stop-based denominator `risk_per_share` and the notional ceiling untouched
(anti-collapse: w is a function of the dimensionless illiq alone, numerator only). High
illiq (illiquid) -> w < 1 (down-weight); low illiq (liquid) -> w > 1.

Frozen derivation rules (candidate.json is the machine home; this prose mirrors it):
  alpha  = 1.0
  WINDOW = 14 prior sessions of daily returns  (needs >= 15 daily priors, the ATR cohort)
  illiq_ref = median(illiq) over the illiq-available closed trades (untreated population)
  w_lo   = illiq_ref / p90(illiq),  w_hi = illiq_ref / p10(illiq)   (numpy-default linear-interp)

Gate 1a (collinearity vs the budget axis): |Pearson r(w(illiq), risk_per_share)| < 0.70.
Gate 1b (collinearity vs the KEPT ratio-ATR tilt): |Pearson r(w(illiq), w_ratio_atr)| < 0.70 —
  the new tilt must be a genuinely new reallocation, not a re-expression of the ratio-ATR
  weight already levered.
Gate 2 (materiality over ALL 167 closed): first-order |RoR' - RoR| >= 0.00065 AND integer
  qty-change fraction >= 0.05 (a floored-to-0 qty counts as a change).

The gate emits ABSOLUTE-value collinearity readings so the `< 0.70` threshold is correct on a
signed statistic (a strongly-negative r must STOP, not spuriously pass).
"""
import json, struct, glob, math, datetime, sys

DATA = "/Users/mini/dev/korea-adapter-sdk-ls/data/turn4-fresh"
V30 = f"{DATA}/runs/20260715T092847Z-backtest-orb-v30/performance.json"
CATALOG = f"{DATA}/catalog/data/bars"
WINDOW = 14
SCALE = 1e9  # nautilus fixed-point raw / 1e9 = price (and volume raw/1e9 = share count)
KST_OFFSET = datetime.timedelta(hours=9)

# frozen constants (v30 manifest)
RISK_PER_TRADE_KRW = 299_340.0
NOTIONAL_PER_POSITION = 10_000_000.0
# frozen KEPT ratio-ATR tilt params (v30 manifest) — for the Gate-1b redundancy check
RATIO_ATR_REF = 0.07315764
RATIO_ATR_W_LO = 0.70269755
RATIO_ATR_W_HI = 1.44548956
# frozen tilt / threshold parameters (pre-register)
ALPHA = 1.0
COLLIN_THRESH = 0.70
ROR_SHIFT_FLOOR = 0.00065
QTY_CHANGE_FLOOR = 0.05

import pyarrow.parquet as pq


def dec(b):
    return struct.unpack("<q", b)[0] / SCALE


def kst_date_from_ns(ns):
    dt = datetime.datetime(1970, 1, 1) + datetime.timedelta(microseconds=ns / 1000)
    return (dt + KST_OFFSET).date()


def load_daily(symbol):
    """Daily OHLC + volume by KST session date (close, volume are what Amihud needs)."""
    by_date = {}
    for f in sorted(glob.glob(f"{CATALOG}/{symbol}-1-DAY-LAST-EXTERNAL/*.parquet")):
        d = pq.read_table(f).to_pydict()
        for i in range(len(d["ts_event"])):
            dt = kst_date_from_ns(d["ts_event"][i])
            by_date[dt] = (
                dec(d["open"][i]), dec(d["high"][i]), dec(d["low"][i]),
                dec(d["close"][i]), dec(d["volume"][i]),
            )
    return by_date


def prior_atr(by_date, session_date, window=WINDOW):
    """Exact port of backtest.rs::prior_atr (u5 precedent) — for the ratio-ATR cross-check."""
    if window == 0:
        return None
    priors = sorted((dt, v) for dt, v in by_date.items() if dt < session_date)
    if len(priors) < window + 1:
        return None
    tail = priors[-(window + 1):]
    sum_tr = 0.0
    for k in range(1, len(tail)):
        prev_close = tail[k - 1][1][3]
        _, high, low, _, _ = tail[k][1]
        sum_tr += max(high - low, abs(high - prev_close), abs(low - prev_close))
    return sum_tr / window


def prior_illiq(by_date, session_date, window=WINDOW):
    """Amihud illiquidity over the prior `window` daily returns strictly before the session.

    Same entry-safe prior-window discipline as prior_atr: only sessions with date <
    session_date; needs >= window+1 daily priors (to form `window` returns). Each day's
    contribution is |close_k/close_{k-1} - 1| / (close_k * volume_k) (KRW turnover); the
    mean over the window is the Amihud measure. Returns None where under-covered or a
    non-positive turnover makes a day undefined.
    """
    priors = sorted((dt, v) for dt, v in by_date.items() if dt < session_date)
    if len(priors) < window + 1:
        return None
    tail = priors[-(window + 1):]
    vals = []
    for k in range(1, len(tail)):
        prev_close = tail[k - 1][1][3]
        close = tail[k][1][3]
        volume = tail[k][1][4]
        turnover = close * volume
        if prev_close <= 0.0 or turnover <= 0.0:
            return None
        ret = abs(close / prev_close - 1.0)
        vals.append(ret / turnover)
    if not vals:
        return None
    return sum(vals) / len(vals)


def pearson(xs, ys):
    n = len(xs)
    mx, my = sum(xs) / n, sum(ys) / n
    cov = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    vx = sum((x - mx) ** 2 for x in xs)
    vy = sum((y - my) ** 2 for y in ys)
    if vx == 0 or vy == 0:
        return float("nan")
    return cov / math.sqrt(vx * vy)


def spearman(xs, ys):
    def ranks(v):
        order = sorted(range(len(v)), key=lambda i: v[i])
        r = [0.0] * len(v)
        i = 0
        while i < len(v):
            j = i
            while j + 1 < len(v) and v[order[j + 1]] == v[order[i]]:
                j += 1
            avg = (i + j) / 2.0 + 1
            for k in range(i, j + 1):
                r[order[k]] = avg
            i = j + 1
        return r
    return pearson(ranks(xs), ranks(ys))


def percentile(sorted_v, q):
    """numpy-default linear interpolation (frozen in the pre-register)."""
    n = len(sorted_v)
    if n == 1:
        return sorted_v[0]
    pos = q * (n - 1)
    lo = math.floor(pos)
    frac = pos - lo
    if lo + 1 >= n:
        return sorted_v[lo]
    return sorted_v[lo] + frac * (sorted_v[lo + 1] - sorted_v[lo])


def clamp_weight(x, ref, w_lo, w_hi, alpha=ALPHA):
    """clamp((ref/x)^alpha, w_lo, w_hi); fail-closed neutral on bad inputs (KTD-5 discipline)."""
    if alpha == 0.0:
        return 1.0
    if x is None or x <= 0.0:
        return 1.0
    raw = (ref / x) ** alpha
    return min(max(raw, w_lo), w_hi)


def qty(budget, rps, px):
    return min(math.floor(budget / rps), math.floor(NOTIONAL_PER_POSITION / px))


def main():
    out_path = sys.argv[-1]
    perf = json.load(open(V30))
    closed = [t for t in perf["trades"] if t.get("ts_closed") is not None]

    # ---- per-trade rows over all closed trades with risk_capital & qty>0 ----
    rows = []  # (sym, sess, rps, rc, r, avg_px_open)
    for t in closed:
        q = t["quantity"]
        rc = t.get("risk_capital")
        if rc is None or q <= 0:
            continue
        sess = kst_date_from_ns(t["ts_opened"])
        rps = rc / q
        rows.append((t["symbol"], sess, rps, rc, t["realized_r"], t["avg_px_open"]))

    # ---- illiq (Amihud) + v (ratio-ATR) per trade ----
    daily_cache = {}
    illiq_by_row = []      # aligned with rows; None where under-covered
    w_ratio_by_row = []    # kept ratio-ATR weight (Gate 1b axis)
    n_no_illiq = 0
    for (sym, sess, rps, rc, r, px) in rows:
        if sym not in daily_cache:
            daily_cache[sym] = load_daily(sym)
        il = prior_illiq(daily_cache[sym], sess)
        if il is None or il <= 0.0:
            illiq_by_row.append(None)
            n_no_illiq += 1
        else:
            illiq_by_row.append(il)
        atr = prior_atr(daily_cache[sym], sess)
        v = (atr / px) if (atr is not None and atr > 0 and px > 0) else None
        w_ratio_by_row.append(clamp_weight(v, RATIO_ATR_REF, RATIO_ATR_W_LO, RATIO_ATR_W_HI))

    il_avail = sorted(v for v in illiq_by_row if v is not None)  # untreated distribution
    n_il = len(il_avail)

    # ---- frozen derivation of illiq_ref and clamps from the untreated distribution ----
    illiq_ref = percentile(il_avail, 0.5)
    p10 = percentile(il_avail, 0.10)
    p90 = percentile(il_avail, 0.90)
    w_lo = illiq_ref / p90
    w_hi = illiq_ref / p10

    # ---- per-trade weight w(illiq) (w = 1 where illiq unavailable, skip-not-reject) ----
    w_by_row = [clamp_weight(il, illiq_ref, w_lo, w_hi) if il is not None else 1.0
                for il in illiq_by_row]

    # ============ GATE 1a — collinearity w(illiq) vs risk_per_share (illiq cohort) ============
    cohort = [i for i in range(len(rows)) if illiq_by_row[i] is not None]
    g_w = [w_by_row[i] for i in cohort]
    g_rps = [rows[i][2] for i in cohort]
    r_rps = pearson(g_w, g_rps)
    rho_rps = spearman(g_w, g_rps)
    collin_abs_rps = abs(r_rps)
    gate1a_go = collin_abs_rps < COLLIN_THRESH

    # ============ GATE 1b — collinearity w(illiq) vs KEPT ratio-ATR weight (illiq cohort) ====
    g_wratio = [w_ratio_by_row[i] for i in cohort]
    r_ratio = pearson(g_w, g_wratio)
    rho_ratio = spearman(g_w, g_wratio)
    collin_abs_ratio_atr = abs(r_ratio)
    gate1b_go = collin_abs_ratio_atr < COLLIN_THRESH

    # ============ GATE 2 — materiality over ALL rows ============
    num = sum(rows[i][3] * rows[i][4] for i in range(len(rows)))
    den = sum(rows[i][3] for i in range(len(rows)))
    ror = num / den
    num_p = sum(w_by_row[i] * rows[i][3] * rows[i][4] for i in range(len(rows)))
    den_p = sum(w_by_row[i] * rows[i][3] for i in range(len(rows)))
    ror_prime = num_p / den_p
    ror_shift = abs(ror_prime - ror)
    cond_a = ror_shift >= ROR_SHIFT_FLOOR

    n_change = 0
    n_to_zero = 0
    for i in range(len(rows)):
        (_, _, rps, _, _, px) = rows[i]
        q_new = qty(RISK_PER_TRADE_KRW * w_by_row[i], rps, px)
        q_old = qty(RISK_PER_TRADE_KRW * 1.0, rps, px)
        if q_new != q_old:
            n_change += 1
            if q_new == 0:
                n_to_zero += 1
    qty_change_frac = n_change / len(rows)
    cond_b = qty_change_frac >= QTY_CHANGE_FLOOR
    gate2_go = cond_a and cond_b

    dual_go = gate1a_go and gate1b_go and gate2_go

    # ---- human-readable report (stdout; the gate reads only the JSON file) ----
    print(f"closed trades:                    {len(closed)}")
    print(f"  with risk_capital & qty>0:      {len(rows)}   (materiality n; all 167)")
    print(f"  illiq-available (>=15 priors):  {n_il}   (excluded {n_no_illiq} — w=1 skip-not-reject)")
    print()
    print("frozen-derived tilt values (untreated illiq distribution):")
    print(f"  illiq (Amihud): min {il_avail[0]:.3e}  p10 {p10:.3e}  median(ref) {illiq_ref:.3e}  p90 {p90:.3e}  max {il_avail[-1]:.3e}")
    print(f"  alpha = {ALPHA}   illiq_ref = {illiq_ref:.6e}   w_lo = {w_lo:.8f}   w_hi = {w_hi:.8f}")
    print(f"  weight span over cohort: min {min(g_w):.6f}  max {max(g_w):.6f}")
    print()
    print("=== GATE 1a — collinearity w(illiq) vs risk_per_share ===")
    print(f"  Pearson r = {r_rps:.4f}  |r| = {collin_abs_rps:.4f}  Spearman = {rho_rps:.4f}   |r|<{COLLIN_THRESH} -> {'GO' if gate1a_go else 'STOP'}")
    print("=== GATE 1b — collinearity w(illiq) vs KEPT ratio-ATR weight ===")
    print(f"  Pearson r = {r_ratio:.4f}  |r| = {collin_abs_ratio_atr:.4f}  Spearman = {rho_ratio:.4f}   |r|<{COLLIN_THRESH} -> {'GO' if gate1b_go else 'STOP'}")
    print("=== GATE 2 — materiality (all 167) ===")
    print(f"  RoR = {ror:.6f}  RoR' = {ror_prime:.6f}  |shift| = {ror_shift:.6f}  >= {ROR_SHIFT_FLOOR} -> {'PASS' if cond_a else 'FAIL'}")
    print(f"  qty-change frac = {qty_change_frac:.4f} ({n_change}/{len(rows)}; {n_to_zero} floored to 0)  >= {QTY_CHANGE_FLOOR} -> {'PASS' if cond_b else 'FAIL'}")
    print()
    print(f"=== PHASE-A DECISION: {'DUAL GO' if dual_go else 'STOP'} "
          f"(1a {'GO' if gate1a_go else 'STOP'}, 1b {'GO' if gate1b_go else 'STOP'}, 2 {'GO' if gate2_go else 'STOP'}) ===")

    # ---- canonical readings artifact (the gate reads THIS) ----
    readings = {
        "collin_abs_rps": round(collin_abs_rps, 4),
        "collin_abs_ratio_atr": round(collin_abs_ratio_atr, 4),
        "ror_shift": round(ror_shift, 6),
        "qty_change_frac": round(qty_change_frac, 4),
    }
    with open(out_path, "w") as fh:
        json.dump(readings, fh, sort_keys=True)


if __name__ == "__main__":
    main()
