"""Vendor Python baseline for the options workloads (options phase, step D2).

Times whole workloads (W4-W6 in workloads.json) through the official
thetadata 1.0.17 package's public methods, returning Polars frames:
synchronously (``--threads 1``) and threaded on one client (``--threads 8``,
the same concurrency as ours on PRO). Run under the same scoped exception and
rules as baseline.py: by the Coordinator only, key from THETADATA_API_KEY,
timings and counts only, never a row value.
"""
from __future__ import annotations

import argparse
import datetime as dt
import importlib.metadata as md
import json
import os
import sys
import time
from pathlib import Path

import log_guard

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "p1"))
from common import Pacer, host, peak_rss_bytes, time_workload  # noqa: E402

log_guard.install()

ROOT = Path(__file__).resolve().parent
PACKAGES = ("thetadata", "polars", "pandas", "grpcio", "protobuf", "zstandard", "numpy")


class Stop(Exception):
    """A limit or availability signal: finish the workload, then stop."""


def status_of(error: Exception) -> str:
    """The gRPC status name only; never the server's text."""
    import grpc

    if isinstance(error, grpc.RpcError):
        return error.code().name
    return "NOT_FOUND" if type(error).__name__ == "NoDataFoundError" else type(error).__name__


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--workloads", required=True, help="comma-separated IDs from workloads.json")
    parser.add_argument("--threads", default="1,8", help="comma-separated thread counts, each run per round")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--min-interval", type=float, default=2.0, help="seconds between workload runs")
    parser.add_argument("--out", type=Path, default=ROOT.parents[1] / "artifacts" / "python-baseline")
    args = parser.parse_args()
    threads = [int(t) for t in args.threads.split(",")]
    if any(not 1 <= t <= 8 for t in threads):
        print("--threads values must be 1 to 8 (PRO concurrency)", file=sys.stderr)
        return 2
    key = os.environ.get("THETADATA_API_KEY")
    if not key:
        print("THETADATA_API_KEY is not set", file=sys.stderr)
        return 2
    try:
        log_guard.self_test()
    except RuntimeError as error:
        print(f"Refusing to run: {error}", file=sys.stderr)
        return 2
    from thetadata import ThetaClient

    workloads = json.loads((ROOT / "workloads.json").read_text())
    selected = [w.strip() for w in args.workloads.split(",") if w.strip()]
    client = ThetaClient(api_key=key, dataframe_type="polars")
    del key
    import_peak_rss = peak_rss_bytes()
    report = {"run_id": dt.datetime.now(dt.timezone.utc).strftime("pyopt-%Y%m%dT%H%M%SZ"),
              "command": "options-vendor", "host": host(),
              "packages": {name: md.version(name) for name in PACKAGES},
              "settings": {k: str(v) for k, v in vars(args).items()},
              "import_peak_rss_bytes": import_peak_rss, "results": [], "stopped": None}
    pacer = Pacer(args.min_interval)
    for round_no in range(args.rounds):
        # Alternate the order of the thread counts between rounds (AB/BA).
        order = threads if round_no % 2 == 0 else list(reversed(threads))
        for workload in selected:
            spec = workloads[workload]
            method = getattr(client, spec["method"])
            for count in order:
                pacer.wait()
                result = time_workload(lambda kw: method(**kw), spec["requests"], count, status_of)
                result |= {"round": round_no, "workload": workload}
                report["results"].append(result)
                statuses = sorted({r["status"] for r in result["requests"]})
                print(f"round {round_no} {workload} x{count}: {result['wall_ns'] / 1e9:.2f} s, "
                      f"{result['rows']} rows, {statuses}", flush=True)
                if {"RESOURCE_EXHAUSTED", "UNAVAILABLE"} & set(statuses):
                    report["stopped"] = f"{workload} x{count}: {statuses}"
                    break
            if report["stopped"]:
                break
        if report["stopped"]:
            break
    report["peak_rss_bytes"] = peak_rss_bytes()
    text = json.dumps(report, indent=2)
    token = getattr(getattr(client, "auth_token", None), "session_uuid", "") or ""
    identity = [value for value in (str(getattr(client, "email", "") or ""), token) if value]
    if any(value in text for value in identity):
        print("Refusing to write the report: it would contain account identity.", file=sys.stderr)
        return 3
    args.out.mkdir(parents=True, exist_ok=True)
    path = args.out / f"{report['run_id']}.json"
    path.write_text(text)
    print(f"Wrote {path}")
    return 0 if report["stopped"] is None else 1


if __name__ == "__main__":
    sys.exit(main())
