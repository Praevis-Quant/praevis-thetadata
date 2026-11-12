"""Ours for the options workloads (options phase, step D2): the same
workloads and calls as bench/python-baseline/options_vendor.py, through this
package's vendor-named methods returning Polars frames, synchronously and
threaded on one client (up to PRO's documented 8 at once).

Run by the Coordinator only. Timings, counts and peak memory; never values.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import polars
import pyarrow

import praevis_thetadata as pt
from common import ROOT, WORKLOADS, Pacer, host, peak_rss_bytes, time_workload, write_report


def status_of(error: Exception) -> str:
    if isinstance(error, pt.QueryFailed):
        kind = error.args[0] if error.args else "unknown"
        code = error.args[2] if len(error.args) > 2 else None
        return f"{kind}:{code}" if code else str(kind)
    return type(error).__name__


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "artifacts" / "p1")
    parser.add_argument("--workloads", required=True)
    parser.add_argument("--threads", default="1,8")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--min-interval", type=float, default=2.0, help="seconds between workload runs")
    args = parser.parse_args()
    threads = [int(t) for t in args.threads.split(",")]
    if any(not 1 <= t <= 8 for t in threads):
        print("--threads values must be 1 to 8 (PRO concurrency)", file=sys.stderr)
        return 2
    workloads = json.loads(WORKLOADS.read_text())
    selected = [w.strip() for w in args.workloads.split(",") if w.strip()]
    client = pt.Client.from_env(dataframe_type="polars")
    report = {"run_id": args.run_id, "command": "options-ours", "host": host(),
              "packages": {"praevis-thetadata": pt.__version__, "polars": polars.__version__,
                           "pyarrow": pyarrow.__version__},
              "tier": client.tier, "default_concurrency": client.default_concurrency,
              "settings": {k: str(v) for k, v in vars(args).items()},
              "import_peak_rss_bytes": peak_rss_bytes(), "results": [], "stopped": None}
    pacer = Pacer(args.min_interval)
    for round_no in range(args.rounds):
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
                if any("ResourceExhausted" in s or "Unavailable" in s for s in statuses):
                    report["stopped"] = f"{workload} x{count}: {statuses}"
                    break
            if report["stopped"]:
                break
        if report["stopped"]:
            break
    report["peak_rss_bytes"] = peak_rss_bytes()
    return write_report(report, args.out / f"{args.run_id}.json")


if __name__ == "__main__":
    raise SystemExit(main())
