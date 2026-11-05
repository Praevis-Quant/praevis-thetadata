"""Options phase: is our memory bounded by configuration under W5?

Runs ONE options workload in ONE mode per process, so each peak belongs to
one run:
* ``parquet`` -- ``to_parquet`` with the job planner: batches stream from
  Rust to the file and are dropped; memory should stay flat as rows grow;
* ``frame``   -- the vendor-named method into a Polars frame: the whole
  result is held, so memory grows with rows (the comparison).

The Parquet file is private under the ignored ``artifacts/`` and deleted
after its size is recorded, unless ``--keep``. Run by the Coordinator only.
Timings, counts, sizes and peak memory; never values.
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

import polars
import pyarrow

import praevis_thetadata as pt
from common import ROOT, WORKLOADS, host, option_kwargs, peak_rss_bytes, write_report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--workload", required=True, help="one workload ID with one request")
    parser.add_argument("--mode", required=True, choices=["parquet", "frame"])
    parser.add_argument("--out", type=Path, default=ROOT / "artifacts" / "p1")
    parser.add_argument("--keep", action="store_true", help="keep the private Parquet file")
    args = parser.parse_args()
    spec = json.loads(WORKLOADS.read_text())[args.workload]
    if len(spec["requests"]) != 1:
        print("--workload must have exactly one request", file=sys.stderr)
        return 2
    kwargs = option_kwargs(spec["requests"][0])
    client = pt.Client.from_env(dataframe_type="polars")
    report = {"run_id": args.run_id, "command": "options-memory", "host": host(),
              "packages": {"praevis-thetadata": pt.__version__, "polars": polars.__version__,
                           "pyarrow": pyarrow.__version__},
              "tier": client.tier, "scheduler": list(client.scheduler),
              "workload": args.workload, "mode": args.mode,
              "import_peak_rss_bytes": peak_rss_bytes(), "stopped": None}
    start = time.perf_counter_ns()
    try:
        if args.mode == "parquet":
            path = args.out / f"{args.run_id}.parquet"
            path.parent.mkdir(parents=True, exist_ok=True)
            rows, row_groups, size = client.to_parquet(spec["method"], path, **kwargs)
            report |= {"rows": rows, "row_groups": row_groups, "file_bytes": size}
            if not args.keep and path.exists():
                path.unlink()
        else:
            frame = getattr(client, spec["method"])(**kwargs)
            report |= {"rows": int(frame.shape[0]), "columns": int(frame.shape[1]),
                       "frame_bytes": int(frame.estimated_size())}
            del frame
        report["status"] = "OK"
    except pt.QueryFailed as error:
        report["status"] = f"{error.args[0]}:{error.args[2] if len(error.args) > 2 else None}"
        report["stopped"] = report["status"]
    report["wall_ns"] = time.perf_counter_ns() - start
    report["peak_rss_bytes"] = peak_rss_bytes()
    print(f"{args.workload} {args.mode}: {report.get('rows')} rows in {report['wall_ns'] / 1e9:.1f} s, "
          f"peak {report['peak_rss_bytes'] / 2**20:.1f} MiB "
          f"(idle {report['import_peak_rss_bytes'] / 2**20:.1f} MiB)", flush=True)
    return write_report(report, args.out / f"{args.run_id}.json")


if __name__ == "__main__":
    raise SystemExit(main())
