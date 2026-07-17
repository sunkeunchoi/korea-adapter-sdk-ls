#!/usr/bin/env python3
"""Independent catalog-wide twin for the gap-retention Phase-A gate.

Unlike diagnostic.py's entry-local reconstruction, this program first builds
catalog-wide daily and opening-range maps and only then attaches the frozen
head-v30 trades. It shares no implementation or intermediate artifact with the
primary diagnostic.
"""

from collections import defaultdict
from datetime import datetime, time, timedelta, timezone
from pathlib import Path
import json
import math
import struct
import sys

import pyarrow.parquet as parquet


PIN = {
    "run_id": "20260715T092847Z-backtest-orb-v30",
    "strategy_id": "orb",
    "strategy_version": 30,
    "strategy_code_hash": "6ae7b9f11707eec7bed42b5380feac27b8a026adbe720ce5db2e83a2aad587c5",
    "catalog_fingerprint": "3b6be31bdf8d29a8d774d42d490020d65753455acfc1b2214a0b13f14b589200",
    "universe_hash": "1e7394ec17d880de86075178305569fb9769ff3b1c025c904e17f53af60035e1",
}
EXPECTED_RANGE = {"start": "20260526", "end": "20260703"}
EXPECTED_CLOSED = 167
OPEN = time(9, 0)
CLOSE = time(9, 20)
KST_DELTA = timedelta(hours=9)
UNIT = 1_000_000_000
ROOT = Path("data/turn4-fresh")
RUN = ROOT / "runs" / PIN["run_id"]
CATALOG = ROOT / "catalog" / "data" / "bars"


def stop(message):
    raise RuntimeError(message)


def date_and_time(nanoseconds):
    utc = datetime(1970, 1, 1) + timedelta(microseconds=nanoseconds / 1000)
    local = utc + KST_DELTA
    return local.date(), local.time()


def won(blob):
    raw = struct.unpack("<q", blob)[0]
    if raw % UNIT:
        stop("catalog contains a fractional KRW/tick price")
    return raw // UNIT


def materialize_dataset(symbols):
    daily = defaultdict(dict)
    opening_lows = defaultdict(dict)
    for symbol in symbols:
        day_dir = CATALOG / f"{symbol}-1-DAY-LAST-EXTERNAL"
        day_files = sorted(day_dir.glob("*.parquet"))
        if not day_files:
            stop(f"daily catalog absent: {symbol}")
        for file_name in day_files:
            batch = parquet.read_table(file_name, columns=["open", "close", "ts_event"]).to_pydict()
            for event, open_blob, close_blob in zip(batch["ts_event"], batch["open"], batch["close"]):
                session, _ = date_and_time(event)
                record = (event, won(open_blob), won(close_blob))
                prior = daily[symbol].get(session)
                if prior is None or event > prior[0]:
                    daily[symbol][session] = record

        minute_dir = CATALOG / f"{symbol}-1-MINUTE-LAST-EXTERNAL"
        minute_files = sorted(minute_dir.glob("*.parquet"))
        if not minute_files:
            stop(f"minute catalog absent: {symbol}")
        for file_name in minute_files:
            batch = parquet.read_table(file_name, columns=["low", "ts_event"]).to_pydict()
            for event, low_blob in zip(batch["ts_event"], batch["low"]):
                session, clock = date_and_time(event)
                if not (OPEN <= clock < CLOSE):
                    continue
                low = won(low_blob)
                prior = opening_lows[(symbol, session)].get(event)
                if prior is not None and prior != low:
                    stop(f"divergent duplicate minute bar: {symbol} {session}")
                opening_lows[(symbol, session)][event] = low
    return daily, opening_lows


def observation(symbol, session, daily, opening_lows):
    today = daily[symbol].get(session)
    if today is None:
        stop(f"today daily context absent: {symbol} {session}")
    earlier = [record for day, record in daily[symbol].items() if day < session]
    if not earlier:
        stop(f"prior daily context absent: {symbol} {session}")
    prior_close = max(earlier, key=lambda record: record[0])[2]
    today_open = today[1]
    if prior_close <= 0:
        stop(f"non-positive prior close: {symbol} {session}")
    if today_open <= prior_close:
        stop(f"non-positive opening gap: {symbol} {session}")
    lows = opening_lows.get((symbol, session))
    if not lows:
        stop(f"opening range absent: {symbol} {session}")
    fraction = (min(lows.values()) - prior_close) / (today_open - prior_close)
    if not math.isfinite(fraction) or fraction > 1.0:
        stop(f"invalid retention: {symbol} {session}")
    return fraction


def main():
    if len(sys.argv) < 2:
        stop("missing readings output path")
    manifest = json.loads((RUN / "manifest.json").read_text(encoding="utf-8"))
    for field, frozen in PIN.items():
        if manifest.get(field) != frozen:
            stop(f"source identity mismatch: {field}")
    if manifest.get("data_range") != EXPECTED_RANGE:
        stop("source identity mismatch: data_range")
    params = manifest.get("params", {})
    if params.get("range_open") != "09:00:00" or params.get("range_minutes") != 20:
        stop("source identity mismatch: opening range")

    document = json.loads((RUN / "performance.json").read_text(encoding="utf-8"))
    trades = [row for row in document.get("trades", []) if row.get("ts_closed") is not None]
    if len(trades) != EXPECTED_CLOSED:
        stop(f"closed population mismatch: {len(trades)}")
    symbols = sorted({row.get("symbol") for row in trades})
    if None in symbols:
        stop("trade symbol absent")
    daily, opening_lows = materialize_dataset(symbols)

    records = []
    joins = set()
    risk_complete = 0
    for trade in trades:
        session, _ = date_and_time(trade.get("ts_opened"))
        join = (trade["symbol"], session)
        if join in joins:
            stop(f"duplicate trade join key: {join}")
        joins.add(join)
        capital = trade.get("risk_capital")
        profit = trade.get("realized_pnl")
        if not isinstance(capital, (int, float)) or not math.isfinite(capital) or capital <= 0:
            stop(f"incomplete risk: {join}")
        if not isinstance(profit, (int, float)) or not math.isfinite(profit):
            stop(f"incomplete realized P&L: {join}")
        risk_complete += 1
        records.append((trade["symbol"], session, observation(trade["symbol"], session, daily, opening_lows), capital, profit))

    kept = [record for record in records if record[2] >= 0.50]
    dropped = [record for record in records if record[2] < 0.50]
    total_capital = sum(record[3] for record in records)
    kept_capital = sum(record[3] for record in kept)
    if total_capital <= 0 or kept_capital <= 0:
        stop("undefined return-on-risk denominator")
    baseline = sum(record[4] for record in records) / total_capital
    counterfactual = sum(record[4] for record in kept) / kept_capital
    exposure = defaultdict(float)
    for symbol, _, _, capital, _ in kept:
        exposure[symbol] += capital
    dominance = max(exposure.values()) / kept_capital

    result = {
        "population_count": len(records),
        "valid_retention_count": len(records),
        "retained_count": len(kept),
        "rejected_count": len(dropped),
        "retained_session_count": len({record[1] for record in kept}),
        "rejected_session_count": len({record[1] for record in dropped}),
        "risk_complete_count": risk_complete,
        "head_ror": round(baseline, 8),
        "retained_ror": round(counterfactual, 8),
        "predicted_ror_shift": round(counterfactual - baseline, 8),
        "retained_max_risk_capital_share": round(dominance, 8),
    }
    if len(result) != 11 or any(not isinstance(value, (int, float)) or not math.isfinite(value) for value in result.values()):
        stop("invalid canonical reading set")
    Path(sys.argv[-1]).write_text(json.dumps(result, sort_keys=True), encoding="utf-8")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
