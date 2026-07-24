#!/usr/bin/env python3
"""Hand-computation fixtures for diagnostic.py + twin.py (plan U2/U3, R2/R3).

Not a frozen input and not on the diagnose path — a standalone reproducibility harness the
author runs to prove the join + counterfactual + change-tally against known-answer fixtures.
Both scripts are invoked exactly as `turn diagnose` invokes them (readings path as argv[-1]),
via LS_PT075_RUN pointed at a synthesized run dir. Run: `python3 fixture_check.py` (exit 0 = pass).
"""
import datetime
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
DAY = 86_400_000_000_000
NS = 1_779_400_000_000_000_000  # arbitrary base ns; per-trade offsets keep KST dates distinct-per-need


def at(off):
    """Absolute ns for a whole-day offset from the base (the common per-symbol/session case)."""
    return NS + off * DAY


def utc_ns(y, mo, d, h, mi=0):
    """Absolute unix-ns for a UTC wall-clock — used to build a cross-UTC-midnight KST case."""
    dt = datetime.datetime(y, mo, d, h, mi, tzinfo=datetime.timezone.utc)
    return int(dt.timestamp()) * 1_000_000_000


def write_run(run_dir, trades, exits):
    """trades: list of (symbol, ts_opened_ns, quantity, risk_capital, realized_r).
    exits:  list of (symbol, ts_event_ns, mfe_r_or_None)."""
    perf = {"trades": [], "equity_curve": [], "summary": {}}
    for (sym, ts, qty, rc, r) in trades:
        t = {
            "symbol": sym, "entry_side": "BUY", "quantity": qty,
            "avg_px_open": 10000.0, "avg_px_close": 10000.0, "realized_pnl": 0.0,
            "ts_opened": ts, "ts_closed": ts + 3600 * 1_000_000_000,
            "fills": [], "realized_r": r,
        }
        if rc is not None:
            t["risk_capital"] = rc
        perf["trades"].append(t)
    with open(os.path.join(run_dir, "performance.json"), "w") as fh:
        json.dump(perf, fh)

    lines = []
    for (sym, ts, mfe) in exits:
        values = {"price": 10000.0, "qty": 1.0, "realized_r": 0.0}
        if mfe is not None:
            values["mfe_r"] = mfe
        lines.append(json.dumps({
            "ts_event": ts,
            "decision_detail": {"kind": "stop_hit", "symbol": sym, "values": values},
        }))
    with open(os.path.join(run_dir, "decisions.jsonl"), "w") as fh:
        fh.write("\n".join(lines) + ("\n" if lines else ""))


def run_script(script, run_dir):
    env = dict(os.environ, LS_PT075_RUN=run_dir)
    out_path = os.path.join(run_dir, f"{script}.out.json")
    proc = subprocess.run(
        [sys.executable, os.path.join(HERE, script), out_path],
        env=env, capture_output=True, text=True,
    )
    readings = None
    if os.path.exists(out_path):
        readings = json.load(open(out_path))
    return proc.returncode, readings, proc.stderr, proc.stdout


def approx(a, b, tol=1e-6):
    return abs(a - b) <= tol


def case_a_hand_computation():
    """3 trades, equal risk_capital=100:
       t1 give-back (mfe 0.9, realized -0.5) -> booked +0.75
       t2 former target (mfe 1.2, realized 1.0) -> booked 0.75 (RoR falls)
       t3 untouched (mfe 0.3, realized 0.2) -> unchanged
       RoR_base = (-0.5+1.0+0.2)/3 = 0.233333 ; RoR_prime = (0.75+0.75+0.2)/3 = 0.566667
       ror_delta = 0.333333 ; exit_change_frac = 2/3."""
    with tempfile.TemporaryDirectory() as d:
        write_run(
            d,
            trades=[("AAA", at(0), 1, 100.0, -0.5), ("BBB", at(0), 1, 100.0, 1.0), ("CCC", at(0), 1, 100.0, 0.2)],
            exits=[("AAA", at(0), 0.9), ("BBB", at(0), 1.2), ("CCC", at(0), 0.3)],
        )
        for script in ("diagnostic.py", "twin.py"):
            rc, rd, err, _ = run_script(script, d)
            assert rc == 0, f"{script} case-a exited {rc}: {err}"
            assert approx(rd["ror_base"], 0.233333, 1e-4), f"{script} ror_base {rd['ror_base']}"
            assert approx(rd["ror_prime"], 0.566667, 1e-4), f"{script} ror_prime {rd['ror_prime']}"
            assert approx(rd["ror_delta"], 0.333333, 1e-4), f"{script} ror_delta {rd['ror_delta']}"
            assert approx(rd["exit_change_frac"], 2 / 3, 1e-4), f"{script} frac {rd['exit_change_frac']}"
    print("case (a) hand computation: PASS (both scripts, ror_delta=0.333333, exit_change_frac=2/3)")


def case_b_zero_join():
    """Trades exist but no exit envelope shares their (symbol, date) -> nonzero exit, clear msg."""
    with tempfile.TemporaryDirectory() as d:
        write_run(
            d,
            trades=[("AAA", at(0), 1, 100.0, -0.5)],
            exits=[("ZZZ", at(5), 0.9)],  # different symbol AND day
        )
        for script in ("diagnostic.py", "twin.py"):
            rc, rd, err, _ = run_script(script, d)
            assert rc != 0, f"{script} case-b should exit nonzero, got {rc}"
            assert "zero join" in err.lower(), f"{script} case-b message unclear: {err!r}"
    print("case (b) zero-join: PASS (both scripts exit nonzero with a clear message)")


def case_c_missing_mfe_excluded():
    """A trade whose exit envelope lacks mfe_r is excluded + counted, never read as 0.
       t1 (mfe 0.9, realized -0.5) counted; t2 exit missing mfe_r -> dropped.
       Only t1 remains: RoR_base=-0.5, RoR_prime=0.75, delta=1.25, frac=1/1.
       Also assert the excluded trade is COUNTED (diagnostic's stdout reports 'excluded 1'),
       not silently dropped — the 'never read as 0' + 'counted' halves of R2(c)."""
    with tempfile.TemporaryDirectory() as d:
        write_run(
            d,
            trades=[("AAA", at(0), 1, 100.0, -0.5), ("BBB", at(0), 1, 100.0, 1.0)],
            exits=[("AAA", at(0), 0.9), ("BBB", at(0), None)],  # BBB exit predates telemetry
        )
        for script in ("diagnostic.py", "twin.py"):
            rc, rd, err, out = run_script(script, d)
            assert rc == 0, f"{script} case-c exited {rc}: {err}"
            # not read as 0: a read-as-0 would give ror_base -0.25 (over both trades), not -0.5.
            assert approx(rd["ror_base"], -0.5, 1e-6), f"{script} ror_base {rd['ror_base']}"
            assert approx(rd["ror_prime"], 0.75, 1e-6), f"{script} ror_prime {rd['ror_prime']}"
            assert approx(rd["exit_change_frac"], 1.0, 1e-6), f"{script} frac {rd['exit_change_frac']}"
        # counted (not silently dropped): the diagnostic reports the exclusion tally.
        _, _, _, diag_out = run_script("diagnostic.py", d)
        assert "excluded 1" in diag_out, f"diagnostic must count the excluded trade: {diag_out!r}"
    print("case (c) missing-mfe excluded: PASS (both scripts drop the telemetry-less exit, counted)")


def case_d_perturbation_moves_both_identically():
    """Nudge one trade's mfe_r across 0.75 -> both scripts move exit_change_frac identically (R3)."""
    with tempfile.TemporaryDirectory() as d:
        # CCC mfe 0.74 -> untouched; frac = 2/3 (AAA,BBB change)
        write_run(
            d,
            trades=[("AAA", at(0), 1, 100.0, -0.5), ("BBB", at(0), 1, 100.0, 1.0), ("CCC", at(0), 1, 100.0, 0.2)],
            exits=[("AAA", at(0), 0.9), ("BBB", at(0), 1.2), ("CCC", at(0), 0.74)],
        )
        diag_lo = run_script("diagnostic.py", d)[1]
        twin_lo = run_script("twin.py", d)[1]
    with tempfile.TemporaryDirectory() as d:
        # CCC mfe 0.76 -> now touched; frac = 3/3 (CCC realized 0.2 -> booked 0.75, a change)
        write_run(
            d,
            trades=[("AAA", at(0), 1, 100.0, -0.5), ("BBB", at(0), 1, 100.0, 1.0), ("CCC", at(0), 1, 100.0, 0.2)],
            exits=[("AAA", at(0), 0.9), ("BBB", at(0), 1.2), ("CCC", at(0), 0.76)],
        )
        diag_hi = run_script("diagnostic.py", d)[1]
        twin_hi = run_script("twin.py", d)[1]
    assert diag_lo == twin_lo, f"below-threshold: diag {diag_lo} vs twin {twin_lo}"
    assert diag_hi == twin_hi, f"above-threshold: diag {diag_hi} vs twin {twin_hi}"
    assert approx(diag_lo["exit_change_frac"], 2 / 3, 1e-4), diag_lo
    assert approx(diag_hi["exit_change_frac"], 1.0, 1e-4), diag_hi
    print("case (d) perturbation: PASS (both scripts move identically across the 0.75 threshold)")


def case_e_kst_offset_is_applied():
    """The join key is the KST (+9h) session date, not the UTC date. Build a trade and its exit
       at UTC instants on DIFFERENT UTC days that fall on the SAME KST day:
         trade opened 2026-05-21 21:00 UTC -> 2026-05-22 06:00 KST
         exit        2026-05-22 03:00 UTC -> 2026-05-22 12:00 KST
       A correct +9h implementation joins them (n=1); a UTC-date implementation would place them
       on 05-21 vs 05-22 and zero-join (exit 2). So this case fails loudly on a wrong offset that
       the same-day cases (where trade and exit share an instant) structurally cannot catch."""
    with tempfile.TemporaryDirectory() as d:
        write_run(
            d,
            trades=[("AAA", utc_ns(2026, 5, 21, 21), 1, 100.0, -0.5)],
            exits=[("AAA", utc_ns(2026, 5, 22, 3), 0.9)],
        )
        for script in ("diagnostic.py", "twin.py"):
            rc, rd, err, _ = run_script(script, d)
            assert rc == 0, f"{script} case-e should join across UTC midnight via KST, got exit {rc}: {err}"
            assert approx(rd["ror_base"], -0.5, 1e-6), f"{script} ror_base {rd['ror_base']}"
            assert approx(rd["ror_prime"], 0.75, 1e-6), f"{script} ror_prime {rd['ror_prime']}"
    print("case (e) KST offset: PASS (both scripts join across UTC midnight on the shared KST date)")


if __name__ == "__main__":
    case_a_hand_computation()
    case_b_zero_join()
    case_c_missing_mfe_excluded()
    case_d_perturbation_moves_both_identically()
    case_e_kst_offset_is_applied()
    print("ALL FIXTURE CHECKS PASS")
