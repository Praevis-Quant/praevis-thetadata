"""End-to-end scenarios through the Python package, against ``--target
emulator`` or ``--target live`` (ADR-0022). The same properties as the Rust
runner: rows arrive, columns equal the registry, errors have the expected
kind. Never values.

    PRAEVIS_EMULATOR=target/debug/praevis-thetadata-emulator \\
        python crates/praevis-thetadata-scenarios/python/scenarios.py --target emulator

**Live runs are manual or Coordinator-run, never CI,** under the approval
rules in AGENTS.md (``--target live --confirm-live``). The key comes from
``THETADATA_API_KEY`` only and is never printed. The report (default under
the ignored ``artifacts/``) holds names, outcomes, codes and column names.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time

import praevis_thetadata as pt

ROOT = pathlib.Path(__file__).resolve().parents[3]
SCHEMAS = json.loads((ROOT / "docs" / "endpoints" / "schemas.json").read_text())
WEEK = {"start_date": dt.date(2024, 6, 3), "end_date": dt.date(2024, 6, 7)}


class Skip(Exception):
    """A scenario that does not apply to this account, with the reason."""


def registered(rpc: str) -> list[str]:
    return [name for name, _ in SCHEMAS[rpc]["columns"]]


def scenarios(client: pt.Client) -> list[tuple[str, object]]:
    def login_tier():
        return f"{client.tier} (default concurrency {client.default_concurrency})", {"tier": client.tier}

    def vendor_named():
        table = client.stock_history_eod("AAPL", **WEEK)
        if table.num_rows == 0:
            raise AssertionError("no rows")
        if table.column_names != registered("GetStockHistoryEod"):
            raise AssertionError(f"columns differ from schemas.json: {table.column_names}")
        return f"{table.num_rows} rows, columns as registered", {"rpc": "GetStockHistoryEod", "code": "OK"}

    def query_job():
        result = client.query("option_history_eod", symbol="SPY", expiration="*", concurrency=2, **WEEK)
        if result.num_rows == 0:
            raise AssertionError("no rows")
        if result.column_names != registered("GetOptionHistoryEod"):
            raise AssertionError(f"columns differ from schemas.json: {result.column_names}")
        return f"{result.num_rows} rows at concurrency 2", {"rpc": "GetOptionHistoryEod", "code": "OK"}

    def batches():
        rows = sum(len(batch) for batch in client.batches("stock_history_eod", symbol="AAPL", **WEEK))
        if rows == 0:
            raise AssertionError("no rows")
        return f"{rows} rows in batches", {}

    def to_parquet():
        with tempfile.TemporaryDirectory() as directory:
            rows, groups, _ = client.to_parquet("stock_history_eod", os.path.join(directory, "s.parquet"),
                                                symbol="AAPL", **WEEK)
        if rows == 0:
            raise AssertionError("no rows written")
        return f"{rows} rows, {groups} row groups", {}

    def tier_refusal():
        try:
            client.query("stock_snapshot_ohlc", symbol="AAPL")
        except pt.QueryFailed as error:
            if error.args[2] == "PermissionDenied":
                return "PermissionDenied", {"rpc": "GetStockSnapshotOhlc", "code": "PermissionDenied"}
            raise AssertionError(f"expected PermissionDenied, got {error.args[0]}/{error.args[2]}") from None
        raise AssertionError("expected a refusal, got rows")

    def not_found():
        kinds = []
        for params in ({"symbol": "NOSUCHSYMBOLX", **WEEK},
                       {"symbol": "AAPL", "start_date": dt.date(2024, 6, 8), "end_date": dt.date(2024, 6, 9)}):
            try:
                client.query("stock_history_eod", **params)
                kinds.append("rows")
            except pt.QueryFailed as error:
                kinds.append(error.args[0])
        if kinds != ["no_data", "no_data"]:
            raise AssertionError(f"expected no_data twice, got {kinds}")
        return "both no_data", {"rpc": "GetStockHistoryEod", "code": "NotFound"}

    def cancel_midway():
        # A long request split in two (option EOD, 366 days a part). It needs
        # options history deeper than FREE's; the package exposes only the
        # account's highest tier, so a refusal means "not entitled": skipped.
        try:
            iterator = client.batches("option_history_eod", symbol="SPY", expiration=dt.date(2024, 12, 20),
                                      start_date=dt.date(2023, 1, 3), end_date=dt.date(2024, 6, 28))
            first = len(next(iter(iterator)))
        except pt.QueryFailed as error:
            if error.args[2] == "PermissionDenied":
                raise Skip("no long-range request entitled on this account (PermissionDenied)") from None
            raise
        iterator.cancel()
        if first == 0:
            raise AssertionError("no first batch")
        return f"{first} rows of the first batch, then cancelled", {"rpc": "GetOptionHistoryEod", "code": "OK"}

    return [("login-tier", login_tier), ("vendor-named-method", vendor_named), ("query-planned", query_job),
            ("batches", batches), ("to-parquet", to_parquet), ("tier-refusal", tier_refusal),
            ("not-found-is-identical", not_found), ("cancel-midway", cancel_midway)]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--target", choices=["emulator", "live"], required=True)
    parser.add_argument("--confirm-live", action="store_true")
    parser.add_argument("--run-id", default="scenarios-py")
    parser.add_argument("--report", type=pathlib.Path)
    args = parser.parse_args()
    process = None
    if args.target == "live":
        if not args.confirm_live:
            print("--target live needs --confirm-live (manual runs only; see docs/live-runbook.md)", file=sys.stderr)
            return 2
        key = os.environ.get("THETADATA_API_KEY")
        if not key:
            print("THETADATA_API_KEY is not set", file=sys.stderr)
            return 2
        client = pt.Client(api_key=key, dataframe_type="arrow")
    else:
        server = os.environ.get("PRAEVIS_EMULATOR")
        if not server:
            print("PRAEVIS_EMULATOR must name the emulator binary", file=sys.stderr)
            return 2
        process = subprocess.Popen([server, "--time-scale", "0.01"], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, text=True)
        urls = json.loads(process.stdout.readline())
        client = pt.Client(api_key="emulator-key", dataframe_type="arrow",
                           _auth_url=urls["auth_url"], _endpoint=urls["endpoint"])
    results = []
    try:
        for name, scenario in scenarios(client):
            started = time.monotonic()
            error_codes = None
            try:
                detail, observed = scenario()
                status = "pass"
            except Skip as reason:
                detail, observed, status = str(reason), None, "skip"
            except pt.QueryFailed as error:
                # Only the kind and the gRPC code: the detail can carry the
                # service's message text, which stays out of every report.
                error_codes = {"kind": error.args[0], "code": error.args[2] if len(error.args) > 2 else None}
                detail = f"QueryFailed kind={error_codes['kind']} code={error_codes['code']}"
                observed, status = None, "fail"
            except Exception as error:  # a scenario's failure is its result
                detail, observed, status = f"{type(error).__name__}: {error}", None, "fail"
            if observed is not None:
                observed = {**observed, "ms": round((time.monotonic() - started) * 1000)}
            print(f"{status:4} {name}: {detail}", file=sys.stderr)
            results.append({"name": name, "status": status, "detail": detail, "observed": observed,
                            "error": error_codes})
    finally:
        if process is not None:
            process.stdin.close()
            process.wait(timeout=30)
            process.stdout.close()
    path = args.report or ROOT / "artifacts" / "scenarios" / f"{args.run_id}.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({"run_id": args.run_id, "target": args.target, "scenarios": results}, indent=2))
    failed = sum(r["status"] == "fail" for r in results)
    print(f"{len(results)} scenarios, {failed} failed; report {path}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
