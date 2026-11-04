"""P1 exit: W1/W2 through this package, measured like the vendor baseline
(bench/python-baseline, run py-20261103T164301Z) for a direct comparison.

* e2e     -- the vendor-named method returning a Polars and a pandas frame
             (network + server + decode + conversion), paced like P0;
* convert -- the baseline's private captures decoded offline by this package
             and converted to Polars and pandas (CPU only), the counterpart of
             the vendor's _convert_response_stream on the same bytes.

Run by the Coordinator only. Timings, counts and peak memory; never values.
"""
from __future__ import annotations

import argparse
import json
import statistics
import time
from pathlib import Path

import pandas
import polars
import pyarrow

import praevis_thetadata as pt
from praevis_thetadata import _native
from common import ROOT, WORKLOADS, Pacer, host, peak_rss_bytes, read_capture, write_report


def e2e(clients: dict, method: str, request: dict, pacer: Pacer) -> dict:
    out = {}
    for kind, client in clients.items():
        pacer.wait()
        start = time.perf_counter_ns()
        try:
            frame = getattr(client, method)(**request)
        except pt.QueryFailed as error:
            out[kind] = {"status": error.args[0] if error.args else "unknown",
                         "grpc_code": error.args[2] if len(error.args) > 2 else None,
                         "ns": time.perf_counter_ns() - start}
            continue
        out[kind] = {"status": "OK", "ns": time.perf_counter_ns() - start,
                     "rows": int(frame.shape[0]), "columns": int(frame.shape[1])}
    return out


def convert(frames: list[bytes], repeats: int) -> dict:
    out = {}
    for kind in ("decode", "polars", "pandas"):
        samples, rows = [], 0
        for _ in range(repeats):
            start = time.perf_counter_ns()
            result = pt.Result(_native._decode_frames(frames))
            if kind == "polars":
                frame = result.to_polars()
            elif kind == "pandas":
                frame = result.to_pandas()
            else:
                frame = None
            samples.append(time.perf_counter_ns() - start)
            rows = result.num_rows if frame is None else int(frame.shape[0])
        out[kind] = {"median_ns": int(statistics.median(samples)), "samples_ns": samples, "rows": rows}
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "artifacts" / "p1")
    parser.add_argument("--workloads", default="W1,W2")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--convert-repeats", type=int, default=5)
    parser.add_argument("--min-interval", type=float, default=3.5,
                        help="seconds between network requests (FREE: 20/min documented)")
    parser.add_argument("--captures", type=Path,
                        help="the baseline's private capture directory, for conversion only")
    parser.add_argument("--skip-network", action="store_true", help="conversion only")
    args = parser.parse_args()
    workloads = json.loads(WORKLOADS.read_text())
    selected = [w.strip() for w in args.workloads.split(",") if w.strip()]
    report = {"run_id": args.run_id, "command": "python-compare", "host": host(),
              "packages": {"praevis-thetadata": pt.__version__, "polars": polars.__version__,
                           "pandas": pandas.__version__, "pyarrow": pyarrow.__version__},
              "settings": {k: str(v) for k, v in vars(args).items()},
              "results": [], "convert": [], "stopped": None}
    if not args.skip_network:
        # One login for both, like the baseline's existing_authorized_client:
        # a second login would end the first client's session.
        polars_client = pt.Client.from_env(dataframe_type="polars")
        clients = {"polars": polars_client,
                   "pandas": pt.Client(existing_authorized_client=polars_client, dataframe_type="pandas")}
        pacer = Pacer(args.min_interval)
        for round_no in range(args.rounds):
            for workload in selected:
                spec = workloads[workload]
                for index, request in enumerate(spec["requests"]):
                    entry = {"round": round_no, "workload": workload, "request": index,
                             "e2e": e2e(clients, spec["method"], request, pacer)}
                    report["results"].append(entry)
                    statuses = {k: v["status"] for k, v in entry["e2e"].items()}
                    print(f"{workload}[{index}] round {round_no}: {statuses}", flush=True)
                    if any(s == "remote" for s in statuses.values()):
                        report["stopped"] = f"{workload}[{index}]: remote status"
                        break
                if report["stopped"]:
                    break
            if report["stopped"]:
                break
    if args.captures:
        for path in sorted(args.captures.glob("*.bin")):
            workload = path.stem.split("-")[0]
            if workload not in selected:
                continue
            entry = {"file": path.name, "workload": workload,
                     **convert(read_capture(path), args.convert_repeats)}
            report["convert"].append(entry)
            print(f"convert {path.name}: polars {entry['polars']['median_ns'] / 1e6:.2f} ms, "
                  f"pandas {entry['pandas']['median_ns'] / 1e6:.2f} ms, rows {entry['polars']['rows']}",
                  flush=True)
    report["peak_rss_bytes"] = peak_rss_bytes()
    return write_report(report, args.out / f"{args.run_id}.json")


if __name__ == "__main__":
    raise SystemExit(main())
