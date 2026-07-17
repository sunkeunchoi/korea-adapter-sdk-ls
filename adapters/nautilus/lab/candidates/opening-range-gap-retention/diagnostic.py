#!/usr/bin/env python3
"""Entry-by-entry Phase-A diagnostic for opening-range gap retention.

This direct-only counterfactual keeps each head-v30 trade's realized P&L and
entry-fixed risk capital, removes rows whose independently reconstructed
retention is below 0.50, and never simulates replacement entries or freed
capacity.  It reads only the pinned manifest/performance pair and matching
daily/one-minute catalog bars.
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


RUN_ID = "20260715T092847Z-backtest-orb-v30"
STRATEGY_VERSION = 30
STRATEGY_HASH = "6ae7b9f11707eec7bed42b5380feac27b8a026adbe720ce5db2e83a2aad587c5"
CATALOG_FINGERPRINT = "3b6be31bdf8d29a8d774d42d490020d65753455acfc1b2214a0b13f14b589200"
UNIVERSE_HASH = "1e7394ec17d880de86075178305569fb9769ff3b1c025c904e17f53af60035e1"
DATA_START = "20260526"
DATA_END = "20260703"
RANGE_OPEN = dt.time(9, 0)
RANGE_END = dt.time(9, 20)
RANGE_MINUTES = 20
POPULATION = 167
CUTOFF = 0.50
SCALE = 1_000_000_000
KST = dt.timezone(dt.timedelta(hours=9))
REPO_ROOT = Path(__file__).resolve().parents[5]
DATA_HOME = REPO_ROOT / "data" / "turn4-fresh"
RUN_HOME = DATA_HOME / "runs" / RUN_ID
BARS_HOME = DATA_HOME / "catalog" / "data" / "bars"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def canonical_price(raw):
    value = struct.unpack("<q", raw)[0]
    require(value % SCALE == 0, "non-integral canonical KRW/tick price")
    return value // SCALE


def kst_stamp(ns):
    return dt.datetime.fromtimestamp(ns / 1_000_000_000, tz=dt.timezone.utc).astimezone(KST)


def rows(files, columns):
    for file_name in sorted(files):
        table = pq.read_table(file_name, columns=columns).to_pydict()
        for index in range(len(table[columns[0]])):
            yield {column: table[column][index] for column in columns}


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
                    bar_type,
                    event,
                    values["open"][index],
                    values["high"][index],
                    values["low"][index],
                    values["close"][index],
                    values["volume"][index],
                    price_precision,
                    size_precision,
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


def reconstruct(entry):
    symbol = entry["symbol"]
    session = kst_stamp(entry["ts_opened"]).date()

    daily_pattern = str(BARS_HOME / f"{symbol}-1-DAY-LAST-EXTERNAL" / "*.parquet")
    daily_files = glob.glob(daily_pattern)
    require(daily_files, f"missing daily bars for {symbol}")
    latest = {}
    for row in rows(daily_files, ["open", "close", "ts_event"]):
        stamp = kst_stamp(row["ts_event"])
        point = (stamp, canonical_price(row["open"]), canonical_price(row["close"]))
        if stamp.date() not in latest or stamp > latest[stamp.date()][0]:
            latest[stamp.date()] = point
    require(session in latest, f"missing today daily bar for {symbol} {session}")
    priors = [point for day, point in latest.items() if day < session]
    require(priors, f"missing prior daily bar for {symbol} {session}")
    prior_close = max(priors, key=lambda point: point[0])[2]
    today_open = latest[session][1]
    require(prior_close > 0, f"non-positive prior close for {symbol} {session}")
    require(today_open > prior_close, f"non-positive opening gap for {symbol} {session}")

    minute_pattern = str(BARS_HOME / f"{symbol}-1-MINUTE-LAST-EXTERNAL" / "*.parquet")
    minute_files = glob.glob(minute_pattern)
    require(minute_files, f"missing minute bars for {symbol}")
    lows_by_timestamp = {}
    for row in rows(minute_files, ["low", "ts_event"]):
        stamp = kst_stamp(row["ts_event"])
        if stamp.date() != session or not (RANGE_OPEN <= stamp.time() < RANGE_END):
            continue
        low = canonical_price(row["low"])
        previous = lows_by_timestamp.get(row["ts_event"])
        require(previous is None or previous == low, f"divergent duplicate minute bar for {symbol} {session}")
        lows_by_timestamp[row["ts_event"]] = low
    require(lows_by_timestamp, f"missing opening range for {symbol} {session}")
    range_low = min(lows_by_timestamp.values())
    retention = (range_low - prior_close) / (today_open - prior_close)
    require(math.isfinite(retention), f"non-finite retention for {symbol} {session}")
    require(retention <= 1.0, f"retention above one for {symbol} {session}")
    return session, retention


def assert_identity(manifest, closed):
    expected = {
        "run_id": RUN_ID,
        "strategy_id": "orb",
        "strategy_version": STRATEGY_VERSION,
        "strategy_code_hash": STRATEGY_HASH,
        "catalog_fingerprint": CATALOG_FINGERPRINT,
        "universe_hash": UNIVERSE_HASH,
    }
    for name, value in expected.items():
        require(manifest.get(name) == value, f"source identity mismatch: {name}")
    require(actual_catalog_fingerprint() == CATALOG_FINGERPRINT, "catalog content fingerprint mismatch")
    require(manifest.get("data_range") == {"start": DATA_START, "end": DATA_END}, "data-range mismatch")
    params = manifest.get("params", {})
    require(params.get("range_open") == "09:00:00", "opening-range start mismatch")
    require(params.get("range_minutes") == RANGE_MINUTES, "opening-range duration mismatch")
    require(len(closed) == POPULATION, f"closed population mismatch: {len(closed)}")


def main():
    require(len(sys.argv) >= 2, "missing readings output path")
    with open(RUN_HOME / "manifest.json", encoding="utf-8") as source:
        manifest = json.load(source)
    with open(RUN_HOME / "performance.json", encoding="utf-8") as source:
        performance = json.load(source)
    closed = [trade for trade in performance.get("trades", []) if trade.get("ts_closed") is not None]
    assert_identity(manifest, closed)

    joined = []
    keys = set()
    for trade in closed:
        key = (trade.get("symbol"), kst_stamp(trade.get("ts_opened")).date())
        require(None not in key, "incomplete trade join key")
        require(key not in keys, f"duplicate trade join key: {key}")
        keys.add(key)
        risk = trade.get("risk_capital")
        pnl = trade.get("realized_pnl")
        require(isinstance(risk, (int, float)) and math.isfinite(risk) and risk > 0, f"incomplete risk: {key}")
        require(isinstance(pnl, (int, float)) and math.isfinite(pnl), f"incomplete realized P&L: {key}")
        session, retention = reconstruct(trade)
        joined.append({"symbol": trade["symbol"], "session": session, "risk": risk, "pnl": pnl, "retention": retention})

    retained = [row for row in joined if row["retention"] >= CUTOFF]
    rejected = [row for row in joined if row["retention"] < CUTOFF]
    all_risk = sum(row["risk"] for row in joined)
    kept_risk = sum(row["risk"] for row in retained)
    require(all_risk > 0 and kept_risk > 0, "undefined return-on-risk denominator")
    head_ror = sum(row["pnl"] for row in joined) / all_risk
    retained_ror = sum(row["pnl"] for row in retained) / kept_risk
    symbol_risk = {}
    for row in retained:
        symbol_risk[row["symbol"]] = symbol_risk.get(row["symbol"], 0.0) + row["risk"]
    max_share = max(symbol_risk.values()) / kept_risk

    readings = {
        "population_count": len(joined),
        "valid_retention_count": len(joined),
        "retained_count": len(retained),
        "rejected_count": len(rejected),
        "retained_session_count": len({row["session"] for row in retained}),
        "rejected_session_count": len({row["session"] for row in rejected}),
        "risk_complete_count": len(joined),
        "head_ror": round(head_ror, 8),
        "retained_ror": round(retained_ror, 8),
        "predicted_ror_shift": round(retained_ror - head_ror, 8),
        "retained_max_risk_capital_share": round(max_share, 8),
    }
    require(len(readings) == 11, "reading contract drift")
    require(all(isinstance(value, (int, float)) and math.isfinite(value) for value in readings.values()), "non-finite reading")
    with open(sys.argv[-1], "w", encoding="utf-8") as sink:
        json.dump(readings, sink, sort_keys=True)
    print(json.dumps(readings, sort_keys=True))


if __name__ == "__main__":
    main()
