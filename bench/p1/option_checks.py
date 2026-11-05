"""Options phase step F: every option endpoint (except the flat files)
through this package's vendor-named methods, with the same requests as
`live_p0 option-checks`. Records status, rows and column types per method;
never a value. Flat files are probed in Rust only (bounded, no download).

Snapshots need an expiration listed today and US market hours. Run by the
Coordinator only, alone (one login per account).
"""
from __future__ import annotations

import argparse
import datetime as dt
import sys
import time
from pathlib import Path

import pyarrow

import praevis_thetadata as pt
from common import ROOT, Pacer, host, write_report

SNAPSHOTS = [
    "option_snapshot_ohlc", "option_snapshot_trade", "option_snapshot_quote",
    "option_snapshot_open_interest", "option_snapshot_market_value",
    "option_snapshot_greeks_implied_volatility", "option_snapshot_greeks_all",
    "option_snapshot_binomial_greeks_all", "option_snapshot_greeks_first_order",
    "option_snapshot_binomial_greeks_first_order", "option_snapshot_greeks_second_order",
    "option_snapshot_binomial_greeks_second_order", "option_snapshot_greeks_third_order",
    "option_snapshot_binomial_greeks_third_order",
]
INTERVAL = [
    "option_history_ohlc", "option_history_quote", "option_history_greeks_all",
    "option_history_binomial_greeks_all", "option_history_greeks_first_order",
    "option_history_binomial_greeks_first_order", "option_history_greeks_second_order",
    "option_history_binomial_greeks_second_order", "option_history_greeks_third_order",
    "option_history_binomial_greeks_third_order", "option_history_greeks_implied_volatility",
]
TICK = [
    "option_history_trade", "option_history_trade_quote", "option_history_trade_greeks_all",
    "option_history_binomial_trade_greeks_all", "option_history_trade_greeks_first_order",
    "option_history_binomial_trade_greeks_first_order", "option_history_trade_greeks_second_order",
    "option_history_binomial_trade_greeks_second_order", "option_history_trade_greeks_third_order",
    "option_history_binomial_trade_greeks_third_order", "option_history_trade_greeks_implied_volatility",
]


def requests(day: dt.date, exp: dt.date, snapshot_exp: dt.date) -> list[tuple[str, dict]]:
    """The 47 non-flat-file option requests, as in `live_p0`'s option set."""
    call = {"symbol": "SPY", "right": "call"}
    out: list[tuple[str, dict]] = [
        ("option_list_symbols", {}),
        ("option_list_dates", {"request_type": "trade", "symbol": "SPY", "expiration": exp, "right": "call"}),
        ("option_list_expirations", {"symbol": "SPY"}),
        ("option_list_strikes", {"symbol": "SPY", "expiration": exp}),
        ("option_list_contracts", {"request_type": "trade", "date": day, "symbol": "SPY"}),
    ]
    out += [(m, {**call, "expiration": snapshot_exp}) for m in SNAPSHOTS]
    out.append(("option_history_eod", {**call, "expiration": exp, "start_date": day, "end_date": day}))
    out += [(m, {**call, "expiration": exp, "date": day, "interval": "1m",
                 "start_time": "10:00:00", "end_time": "10:05:00"}) for m in INTERVAL]
    out += [(m, {**call, "expiration": exp, "date": day,
                 "start_time": "10:00:00", "end_time": "10:01:00"}) for m in TICK]
    out.append(("option_history_open_interest", {**call, "expiration": exp, "date": day}))
    for m in ("option_history_greeks_eod", "option_history_binomial_greeks_eod"):
        out.append((m, {**call, "expiration": exp, "start_date": day, "end_date": day}))
    for m in ("option_at_time_trade", "option_at_time_quote"):
        out.append((m, {**call, "expiration": exp, "start_date": day, "end_date": day, "time_of_day": "10:00:00"}))
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "artifacts" / "p1")
    parser.add_argument("--date", default="2024-06-14")
    parser.add_argument("--expiration", default="2024-06-21")
    parser.add_argument("--snapshot-expiration", required=True, help="a SPY expiration listed today")
    parser.add_argument("--min-interval", type=float, default=1.0)
    args = parser.parse_args()
    day, exp = dt.date.fromisoformat(args.date), dt.date.fromisoformat(args.expiration)
    snapshot_exp = dt.date.fromisoformat(args.snapshot_expiration)
    client = pt.Client.from_env(dataframe_type="arrow")
    report = {"run_id": args.run_id, "command": "option-checks-python", "host": host(),
              "packages": {"praevis-thetadata": pt.__version__, "pyarrow": pyarrow.__version__},
              "tier": client.tier, "results": [], "stopped": None}
    pacer = Pacer(args.min_interval)
    for method, kwargs in requests(day, exp, snapshot_exp):
        pacer.wait()
        start = time.perf_counter_ns()
        entry: dict = {"method": method}
        try:
            table = getattr(client, method)(**kwargs)
            entry |= {"status": "OK", "rows": table.num_rows,
                      "columns": [[f.name, str(f.type)] for f in table.schema]}
        except pt.QueryFailed as error:
            entry |= {"status": str(error.args[0]),
                      "grpc_code": error.args[2] if len(error.args) > 2 else None}
        entry["ns"] = time.perf_counter_ns() - start
        report["results"].append(entry)
        print(f"{method:<52} {entry['status']:<12} rows {entry.get('rows')}", flush=True)
        if entry.get("grpc_code") in ("ResourceExhausted", "Unavailable"):
            report["stopped"] = f"{method}: {entry['grpc_code']}"
            break
    return write_report(report, args.out / f"{args.run_id}.json")


if __name__ == "__main__":
    raise SystemExit(main())
