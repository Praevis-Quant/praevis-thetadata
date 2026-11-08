"""The stock, index, calendar and interest-rate checks through this package's
vendor-named methods, with the same requests as `live_p0 entitlements --set
stock-index-rate` (one run approved by the user on 2026-11-07; re-running
needs approval). Records status, rows and column types per method; never a
market value. Calendar results keep their decoded rows (calendar data, not
market prices), in the private report only. Flat files are probed in Rust
only (bounded, no download).

Snapshots return data only during US market hours. Run by the Coordinator
only, alone (one login per account).
"""
from __future__ import annotations

import argparse
import datetime as dt
import time
from pathlib import Path

import pyarrow

import praevis_thetadata as pt
from common import ROOT, Pacer, host, write_report

# As in live_p0: a normal weekday, a weekend day, a full holiday and an early
# close, then two whole years.
CALENDAR_DATES = [
    ("2025-11-05", "weekday"),
    ("2025-11-08", "weekend"),
    ("2025-12-25", "holiday"),
    ("2025-11-28", "early close"),
]
CALENDAR_YEARS = ["2025", "2026"]


def requests(day: dt.date) -> list[tuple[str, dict, str | None]]:
    """(method, keyword arguments, label): the 26 non-flat-file methods, with
    the calendar's 4 dates and 2 years, so 30 requests."""
    tick = {"date": day, "start_time": "10:00:00", "end_time": "10:01:00"}
    interval = {"date": day, "interval": "1m", "start_time": "10:00:00", "end_time": "10:05:00"}
    one_day = {"start_date": day, "end_date": day}
    out: list[tuple[str, dict, str | None]] = [
        ("stock_list_symbols", {}, None),
        ("stock_list_dates", {"request_type": "trade", "symbol": "AAPL"}, None),
        ("stock_snapshot_ohlc", {"symbol": "AAPL"}, None),
        ("stock_snapshot_trade", {"symbol": "AAPL"}, None),
        ("stock_snapshot_quote", {"symbol": "AAPL"}, None),
        ("stock_snapshot_market_value", {"symbol": "AAPL"}, None),
        ("stock_history_eod", {"symbol": "AAPL", **one_day}, None),
        ("stock_history_ohlc", {"symbol": "AAPL", **interval}, None),
        ("stock_history_trade", {"symbol": "AAPL", **tick}, None),
        ("stock_history_quote", {"symbol": "AAPL", **interval}, None),
        ("stock_history_trade_quote", {"symbol": "AAPL", **tick}, None),
        ("stock_at_time_trade", {"symbol": "AAPL", **one_day, "time_of_day": "10:00:00"}, None),
        ("stock_at_time_quote", {"symbol": "AAPL", **one_day, "time_of_day": "10:00:00"}, None),
        ("index_list_symbols", {}, None),
        ("index_list_dates", {"symbol": "SPX"}, None),
        ("index_snapshot_ohlc", {"symbol": "SPX"}, None),
        ("index_snapshot_price", {"symbol": "SPX"}, None),
        ("index_snapshot_market_value", {"symbol": "SPX"}, None),
        ("index_history_eod", {"symbol": "SPX", **one_day}, None),
        ("index_history_ohlc", {"symbol": "SPX", **one_day, "interval": "1m",
                                "start_time": "10:00:00", "end_time": "10:05:00"}, None),
        ("index_history_price", {"symbol": "SPX", **interval}, None),
        ("index_at_time_price", {"symbol": "SPX", **one_day, "time_of_day": "10:00:00"}, None),
        ("calendar_open_today", {}, None),
    ]
    out += [("calendar_on_date", {"date": dt.date.fromisoformat(d)}, f"{d} ({kind})")
            for d, kind in CALENDAR_DATES]
    out += [("calendar_year", {"year": year}, year) for year in CALENDAR_YEARS]
    out.append(("interest_rate_history_eod", {"symbol": "SOFR", **one_day}, None))
    return out


def rows_as_json(table: pyarrow.Table) -> list[dict]:
    """Decoded rows as JSON-safe values: decimals and times as exact text."""
    return [{k: (v if v is None or isinstance(v, (bool, int, str)) else str(v)) for k, v in row.items()}
            for row in table.to_pylist()]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "artifacts" / "p1")
    parser.add_argument("--date", default="2024-06-14")
    parser.add_argument("--min-interval", type=float, default=3.5)
    args = parser.parse_args()
    day = dt.date.fromisoformat(args.date)
    client = pt.Client.from_env(dataframe_type="arrow")
    report = {"run_id": args.run_id, "command": "stock-index-rate-checks-python", "host": host(),
              "packages": {"praevis-thetadata": pt.__version__, "pyarrow": pyarrow.__version__},
              "tier": client.tier, "results": [], "stopped": None}
    pacer = Pacer(args.min_interval)
    for method, kwargs, label in requests(day):
        pacer.wait()
        start = time.perf_counter_ns()
        entry: dict = {"method": method}
        if label:
            entry["label"] = label
        try:
            table = getattr(client, method)(**kwargs)
            entry |= {"status": "OK", "rows": table.num_rows,
                      "columns": [[f.name, str(f.type)] for f in table.schema]}
            if method.startswith("calendar_"):
                entry["calendar_rows"] = rows_as_json(table)
        except pt.QueryFailed as error:
            entry |= {"status": str(error.args[0]),
                      "grpc_code": error.args[2] if len(error.args) > 2 else None}
        entry["ns"] = time.perf_counter_ns() - start
        report["results"].append(entry)
        print(f"{method:<32} {label or '':<26} {entry['status']:<12} rows {entry.get('rows')}", flush=True)
        if entry.get("grpc_code") in ("ResourceExhausted", "Unavailable"):
            report["stopped"] = f"{method}: {entry['grpc_code']}"
            break
    return write_report(report, args.out / f"{args.run_id}.json")


if __name__ == "__main__":
    raise SystemExit(main())
