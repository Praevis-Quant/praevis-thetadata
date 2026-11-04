"""P1 exit criterion a, Python side: the 10 FREE-entitled endpoints through
the package's vendor-named methods (one paced request each). Records status,
rows and the Arrow schema (names and types), never values.

Run by the Coordinator only, with the key in THETADATA_API_KEY (or the other
ADR-0020 sources). See README.md.
"""
from __future__ import annotations

import argparse
import datetime as dt
import time
from pathlib import Path

import praevis_thetadata as pt
from common import ROOT, Pacer, host, write_report

DAY = dt.date(2024, 6, 14)
EXPIRATION = dt.date(2024, 6, 21)
# The parameters of the entitlement map (p1-entitlements-001).
CHECKS = [
    ("stock_history_eod", dict(symbol="AAPL", start_date=DAY, end_date=DAY)),
    ("option_history_eod", dict(start_date=DAY, end_date=DAY, symbol="SPY", expiration=EXPIRATION)),
    ("stock_list_symbols", {}),
    ("stock_list_dates", dict(request_type="trade", symbol="AAPL")),
    ("option_list_symbols", {}),
    ("option_list_expirations", dict(symbol="SPY")),
    ("option_list_strikes", dict(symbol="SPY", expiration=EXPIRATION)),
    ("index_list_symbols", {}),
    ("index_history_eod", dict(symbol="SPX", start_date=DAY, end_date=DAY)),
    ("interest_rate_history_eod", dict(symbol="SOFR", start_date=DAY, end_date=DAY)),
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "artifacts" / "p1")
    parser.add_argument("--min-interval", type=float, default=3.5)
    args = parser.parse_args()
    client = pt.Client.from_env(dataframe_type="arrow")
    pacer = Pacer(args.min_interval)
    results = []
    for method, params in CHECKS:
        pacer.wait()
        start = time.perf_counter_ns()
        try:
            table = getattr(client, method)(**params)
        except pt.QueryFailed as error:
            kind = error.args[0] if error.args else "unknown"
            code = error.args[2] if len(error.args) > 2 else None
            results.append({"method": method, "status": kind, "grpc_code": code,
                            "ns": time.perf_counter_ns() - start})
            print(f"{method:<28} {kind}", flush=True)
            if code in ("ResourceExhausted", "Unavailable"):
                break
            continue
        elapsed = time.perf_counter_ns() - start
        schema = [[field.name, str(field.type)] for field in table.schema]
        results.append({"method": method, "status": "OK", "ns": elapsed,
                        "rows": table.num_rows, "columns": schema})
        print(f"{method:<28} OK rows {table.num_rows}", flush=True)
    report = {"run_id": args.run_id, "command": "python-live-checks", "host": host(),
              "package": pt.__version__, "results": results}
    return write_report(report, args.out / f"{args.run_id}.json")


if __name__ == "__main__":
    raise SystemExit(main())
