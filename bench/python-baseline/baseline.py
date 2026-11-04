"""Vendor Python baseline (performance plan C.1). See README.md before running.

Runs the FREE-tier workloads through the official thetadata 1.0.17 package and
records, per request:

* ``e2e``    -- the public wrapper call, returning a Polars and a pandas
                DataFrame (network + server + conversion);
* ``drain``  -- the same request through the package's own generated gRPC stub,
                consuming the stream without conversion (network + server);
* ``convert``-- the drained responses replayed offline through the package's
                converter, for Polars and pandas (CPU only).

This file may only be executed under the scoped exception in CLAUDE.md: by the
Coordinator, on the user's machine, from the hash-pinned virtual environment,
with the key taken from ``THETADATA_API_KEY``. It never prints or stores the key
or any row value. Output is timings and counts; optional raw captures are written
only under the ignored ``artifacts/`` directory.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.metadata as md
import json
import os
import platform
import statistics
import sys
import time
from pathlib import Path

import log_guard

# Peak resident memory, read the same way as the P1 runners (criterion d).
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "p1"))
from common import peak_rss_bytes  # noqa: E402

# Before anything can import thetadata: its auth logging includes the session
# token (research: "Logging hazard"). See log_guard.py.
log_guard.install()

ROOT = Path(__file__).resolve().parent
PACKAGES = ("thetadata", "polars", "pandas", "grpcio", "protobuf", "zstandard", "numpy")
PROD_TARGET = "mdds-01.thetadata.us:443"


class Stop(Exception):
    """A server response that must end the run (politeness/limit policy)."""


def _date(value: str) -> dt.date:
    return dt.date.fromisoformat(value)


def wrapper_kwargs(method: str, request: dict) -> dict:
    kwargs = dict(request)
    for key in ("start_date", "end_date"):
        kwargs[key] = _date(kwargs[key])
    return kwargs


def build_request(method: str, request: dict, auth_token, email):
    """The exact protobuf request the public wrapper builds (catalogue 1.0.17)."""
    from thetadata._proto.endpoints_pb2 import ContractSpec
    from thetadata._proto.v3grpc import endpoints_pb2 as pb

    params = {"client": "python", **{k: str(v) for k, v in request.items()}}
    info = pb.QueryInfo(auth_token=auth_token, email_hint=email, query_parameters=params)
    if method == "stock_history_eod":
        query = pb.StockHistoryEodRequestQuery(
            symbol=request["symbol"],
            start_date=request["start_date"],
            end_date=request["end_date"],
        )
        return "GetStockHistoryEod", pb.StockHistoryEodRequest(query_info=info, params=query)
    if method == "option_history_eod":
        spec = ContractSpec(
            symbol=request["symbol"],
            expiration=request["expiration"],
            strike=request.get("strike", "*"),
            right=request.get("right", "both"),
        )
        query = pb.OptionHistoryEodRequestQuery(
            contract_spec=spec,
            start_date=request["start_date"],
            end_date=request["end_date"],
        )
        return "GetOptionHistoryEod", pb.OptionHistoryEodRequest(query_info=info, params=query)
    raise ValueError(f"no drain builder for {method}")


def classify(error: Exception) -> str:
    """Status name only; never the server's message text."""
    import grpc

    if isinstance(error, grpc.RpcError):
        code = error.code()
        if code == grpc.StatusCode.RESOURCE_EXHAUSTED:
            raise Stop("RESOURCE_EXHAUSTED") from None
        if code == grpc.StatusCode.NOT_FOUND:
            return "NOT_FOUND"
        raise Stop(code.name) from None
    name = type(error).__name__
    if name == "NoDataFoundError":
        return "NOT_FOUND"
    raise Stop(name) from None


class Pacer:
    """At most one network request per ``interval`` seconds."""

    def __init__(self, interval: float):
        self.interval = interval
        self.last = 0.0

    def wait(self):
        delay = self.last + self.interval - time.monotonic()
        if delay > 0:
            time.sleep(delay)
        self.last = time.monotonic()


def rows_of(frame) -> tuple[int, int]:
    shape = frame.shape
    return int(shape[0]), int(shape[1])


def run_e2e(clients, method, request, pacer):
    out = {}
    for kind, client in clients.items():
        pacer.wait()
        start = time.perf_counter_ns()
        try:
            frame = getattr(client, method)(**wrapper_kwargs(method, request))
        except Exception as error:  # noqa: BLE001 - classified, then stop or record
            out[kind] = {"status": classify(error), "ns": time.perf_counter_ns() - start}
            continue
        elapsed = time.perf_counter_ns() - start
        rows, cols = rows_of(frame)
        out[kind] = {"status": "OK", "ns": elapsed, "rows": rows, "columns": cols}
    return out


def run_drain(stub, method, request, client, pacer):
    rpc, message = build_request(method, request, client.auth_token, client.email)
    pacer.wait()
    responses, sizes = [], []
    start = time.perf_counter_ns()
    first = None
    try:
        for response in getattr(stub, rpc)(message):
            if first is None:
                first = time.perf_counter_ns()
            responses.append(response)
            sizes.append(response.ByteSize())
    except Exception as error:  # noqa: BLE001
        return {"status": classify(error), "ns": time.perf_counter_ns() - start}, []
    end = time.perf_counter_ns()
    return {
        "status": "OK",
        "ns": end - start,
        "first_message_ns": (first or end) - start,
        "messages": len(responses),
        "bytes": sum(sizes),
        "max_message_bytes": max(sizes, default=0),
    }, responses


def run_convert(clients, responses, repeats):
    out = {}
    for kind, client in clients.items():
        samples = []
        rows = cols = 0
        for _ in range(repeats):
            start = time.perf_counter_ns()
            frame = client._convert_response_stream(iter(responses))
            samples.append(time.perf_counter_ns() - start)
            rows, cols = rows_of(frame)
        out[kind] = {"median_ns": int(statistics.median(samples)), "samples_ns": samples,
                     "rows": rows, "columns": cols}
    return out


def save_capture(directory: Path, label: str, responses) -> dict:
    """Length-prefixed serialized responses, private under artifacts/."""
    directory.mkdir(parents=True, exist_ok=True)
    data = b"".join(len(r.SerializeToString()).to_bytes(4, "big") + r.SerializeToString()
                    for r in responses)
    path = directory / f"{label}.bin"
    path.write_bytes(data)
    return {"file": path.name, "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}


def summarize(report: dict) -> None:
    """Medians per workload: no row values, only timings and counts."""
    by_workload: dict[str, list] = {}
    for entry in report["results"]:
        by_workload.setdefault(entry["workload"], []).append(entry)
    for workload, entries in by_workload.items():
        def med(values):
            values = [v for v in values if v is not None]
            return statistics.median(values) / 1e6 if values else float("nan")
        drains = [e["drain"] for e in entries if e.get("drain", {}).get("status") == "OK"]
        line = [f"{workload}: {len(entries)} request runs"]
        if drains:
            line.append(f"drain median {med([d['ns'] for d in drains]):.1f} ms "
                        f"(first message {med([d['first_message_ns'] for d in drains]):.1f} ms, "
                        f"{statistics.median(d['messages'] for d in drains):.0f} messages)")
        for kind in ("polars", "pandas"):
            e2e = [e["e2e"][kind] for e in entries if e.get("e2e", {}).get(kind, {}).get("status") == "OK"]
            conv = [e["convert"][kind]["median_ns"] for e in entries if "convert" in e]
            if e2e:
                rows = statistics.median(x["rows"] for x in e2e)
                line.append(f"{kind}: e2e median {med([x['ns'] for x in e2e]):.1f} ms, "
                            f"convert median {med(conv):.2f} ms, {rows:.0f} rows")
        print("; ".join(line))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--workloads", default="W1,W2", help="comma-separated IDs from workloads.json")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--convert-repeats", type=int, default=5)
    parser.add_argument("--min-interval", type=float, default=3.5,
                        help="seconds between network requests (FREE: ~20/min documented)")
    parser.add_argument("--out", type=Path, default=ROOT.parents[1] / "artifacts" / "python-baseline")
    parser.add_argument("--save-captures", action="store_true",
                        help="write raw responses (private) for the Rust replay comparison")
    parser.add_argument("--target", default=PROD_TARGET)
    args = parser.parse_args()

    key = os.environ.get("THETADATA_API_KEY")
    if not key:
        print("THETADATA_API_KEY is not set", file=sys.stderr)
        return 2
    try:
        log_guard.self_test()
    except RuntimeError as error:
        print(f"Refusing to run: {error}", file=sys.stderr)
        return 2
    import grpc
    from thetadata import ThetaClient
    from thetadata._proto.v3grpc import endpoints_pb2_grpc

    workloads = json.loads((ROOT / "workloads.json").read_text())
    selected = [w.strip() for w in args.workloads.split(",") if w.strip()]
    run_id = dt.datetime.now(dt.timezone.utc).strftime("py-%Y%m%dT%H%M%SZ")
    captures = args.out / run_id
    polars_client = ThetaClient(api_key=key, dataframe_type="polars")
    del key
    pandas_client = ThetaClient(existing_authorized_client=polars_client, dataframe_type="pandas")
    clients = {"polars": polars_client, "pandas": pandas_client}
    stub = endpoints_pb2_grpc.BetaThetaTerminalStub(
        grpc.secure_channel(args.target, grpc.ssl_channel_credentials()))
    pacer = Pacer(args.min_interval)

    report = {
        "run_id": run_id,
        "host": {"platform": platform.platform(), "machine": platform.machine(),
                 "processor": platform.processor(), "python": sys.version.split()[0]},
        "packages": {name: md.version(name) for name in PACKAGES},
        "requirements_sha256": hashlib.sha256((ROOT / "requirements.txt").read_bytes()).hexdigest(),
        "settings": vars(args) | {"out": str(args.out)},
        "results": [],
        "stopped": None,
    }
    try:
        for round_no in range(args.rounds):
            for workload in selected:
                spec = workloads[workload]
                for index, request in enumerate(spec["requests"]):
                    entry = {"round": round_no, "workload": workload, "request": index}
                    # Alternate the order of e2e and drain between rounds (AB/BA).
                    order = ("e2e", "drain") if round_no % 2 == 0 else ("drain", "e2e")
                    responses = []
                    for mode in order:
                        if mode == "e2e":
                            entry["e2e"] = run_e2e(clients, spec["method"], request, pacer)
                        else:
                            entry["drain"], responses = run_drain(
                                stub, spec["method"], request, polars_client, pacer)
                    if responses:
                        entry["convert"] = run_convert(clients, responses, args.convert_repeats)
                        if args.save_captures and round_no == 0:
                            entry["capture"] = save_capture(captures, f"{workload}-{index}", responses)
                    report["results"].append(entry)
                    statuses = {"drain": entry.get("drain", {}).get("status")}
                    statuses |= {f"e2e-{k}": v.get("status") for k, v in entry.get("e2e", {}).items()}
                    print(f"{workload}[{index}] round {round_no}: {statuses}", flush=True)
    except Stop as stop:
        report["stopped"] = str(stop)
        print(f"Stopped on {stop}: the vendor signalled a limit or an error. Results so far are saved.",
              file=sys.stderr)
    report["peak_rss_bytes"] = peak_rss_bytes()
    text = json.dumps(report, indent=2)
    # The report holds timings and counts only. Refuse to write it if the
    # account email or the session token appear anywhere in it.
    token = getattr(getattr(polars_client, "auth_token", None), "session_uuid", "") or ""
    identity = [value for value in (str(getattr(polars_client, "email", "") or ""), token) if value]
    if any(value in text for value in identity):
        print("Refusing to write the report: it would contain account identity.", file=sys.stderr)
        return 3
    args.out.mkdir(parents=True, exist_ok=True)
    path = args.out / f"{run_id}.json"
    path.write_text(text)
    print(f"Wrote {path}")
    summarize(report)
    return 0 if report["stopped"] is None else 1


if __name__ == "__main__":
    sys.exit(main())
