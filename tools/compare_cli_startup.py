"""Compare two release CLIs on one host using synthetic data and alternating order."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import threading
import time
import uuid

from benchmark_auth import IdentityFixture, KEY, TOKEN, ThreadingHTTPServer, summarize


def compare(before, after, samples, warmups, stage_binaries=False):
    binaries = {"before": before.resolve(strict=True), "after": after.resolve(strict=True)}
    # Inherited credentials, proxy settings and worker overrides must not change the fixture.
    environment = {k: v for k, v in os.environ.items()
                   if not k.upper().startswith("THETADATA_")
                   and k.upper() not in {"TOKIO_WORKER_THREADS", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"}}
    profiles = {label: "bench-" + uuid.uuid4().hex for label in binaries}
    missing_profiles = {label: "bench-" + uuid.uuid4().hex for label in binaries}
    timings = {}
    storage = {}
    server = ThreadingHTTPServer(("127.0.0.1", 0), IdentityFixture)
    thread = threading.Thread(target=lambda: server.serve_forever(poll_interval=0.05), daemon=True)
    thread.start()
    stopped = False
    auth = ["--auth-url", f"http://127.0.0.1:{server.server_port}/auth", "--insecure", "--json"]

    with tempfile.TemporaryDirectory(prefix="thetadata-bench-") as cwd:
        executables = binaries.copy()
        if stage_binaries:
            # Useful for WSL: run both byte-identical binaries from native /tmp
            # instead of measuring Windows-mounted filesystem execution overhead.
            for label, binary in binaries.items():
                executables[label] = Path(cwd) / (label + binary.suffix)
                shutil.copy2(binary, executables[label])

        def invoke(label, arguments, *, login=False, expected=0, parse=True, profile=None):
            command = [str(executables[label])]
            if profile is not None:
                command += ["auth", "--profile", profile, "--environment", "stage"]
            command += arguments
            child_env = environment.copy()
            if login:
                child_env["THETADATA_API_KEY"] = KEY
            start = time.perf_counter_ns()
            result = subprocess.run(command, env=child_env, cwd=cwd, capture_output=True, timeout=40)
            elapsed = (time.perf_counter_ns() - start) / 1_000_000
            if any(secret.encode() in result.stdout + result.stderr for secret in (TOKEN, KEY)):
                raise RuntimeError("CLI exposed a synthetic secret; output withheld")
            if result.returncode != expected:
                raise RuntimeError(f"{label} CLI returned {result.returncode}, expected {expected}; output withheld")
            return elapsed, json.loads(result.stdout) if parse else result.stdout.decode()

        def measure(name, operation, validate):
            values = {label: [] for label in binaries}
            for index in range(warmups + samples):
                # AB, BA pairs avoid consistently favoring the later binary.
                order = ("before", "after") if index % 2 == 0 else ("after", "before")
                for label in order:
                    elapsed, data = operation(label)
                    validate(data)
                    if index >= warmups:
                        values[label].append(elapsed)
            before_stats, after_stats = (summarize(values[label]) for label in binaries)
            timings[name] = {
                "before": before_stats, "after": after_stats, "raw_ms": values,
                "median_speedup": before_stats["p50"] / after_stats["p50"],
                "median_reduction_percent": 100 * (1 - after_stats["p50"] / before_stats["p50"]),
            }

        def require(condition):
            if not condition:
                raise RuntimeError("Benchmark response failed validation")

        try:
            measure("auth_and_save_ms", lambda label: invoke(label, auth, login=True, profile=profiles[label]),
                    lambda data: require(data.get("authenticated") is True and data.get("persisted") is True
                                         and data.get("user", {}).get("email") == "benchmark@example.invalid"))
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()
            stopped = True
            # These cases must work after the identity fixture is gone, without login credentials.
            measure("persisted_status_ms", lambda label: invoke(label, ["status", "--json"], profile=profiles[label]),
                    lambda data: require(data.get("persisted") is True and data.get("validity") == "not_checked"
                                         and data.get("user", {}).get("email") == "benchmark@example.invalid"))
            measure("missing_status_ms", lambda label: invoke(label, ["status", "--json"], expected=3,
                                                              profile=missing_profiles[label]),
                    lambda data: require(data.get("persisted") is False))
            measure("absent_logout_ms", lambda label: invoke(label, ["logout", "--json"], profile=missing_profiles[label]),
                    lambda data: require(data.get("removed") is False))
            measure("help_ms", lambda label: invoke(label, ["--help"], parse=False),
                    lambda data: require("Usage:" in data))
            measure("version_ms", lambda label: invoke(label, ["--version"], parse=False),
                    lambda data: require(data.startswith(("theta ", "praevis-thetadata "))))
            for label in binaries:
                _, data = invoke(label, ["status", "--json"], profile=profiles[label])
                storage[label] = data["storage"]
        finally:
            if not stopped:
                server.shutdown()
                thread.join(timeout=5)
                server.server_close()
            # Attempt every cleanup even if one backend operation fails.
            failures = []
            for label in binaries:
                for profile in (profiles[label], missing_profiles[label]):
                    try:
                        invoke(label, ["logout", "--json"], profile=profile)
                    except Exception as error:
                        failures.append(f"{label}/{profile}: {error}")
            if failures:
                raise RuntimeError("Synthetic profile cleanup failed: " + "; ".join(failures))
    return {"schema_version": 1, "measured_at_utc": datetime.now(timezone.utc).isoformat(),
            "platform": platform.platform(), "python": platform.python_version(),
            "logical_processors": os.cpu_count(), "tokio_worker_threads_override": None,
            "executable_location": "temporary directory" if stage_binaries else "original paths",
            "includes_process_startup": True, "transport": "localhost HTTP fixture; no TLS",
            "live_vendor_calls": False, "pair_order": "AB, BA alternating",
            "warmup_iterations_per_case": warmups, "samples_per_binary_per_case": samples,
            "storage": storage, "latency": timings,
            "binaries": {label: {"sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                         for label, path in binaries.items()}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--before-provenance", type=Path, required=True)
    parser.add_argument("--after-provenance", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=100)
    parser.add_argument("--warmups", type=int, default=5)
    parser.add_argument("--stage-binaries", action="store_true", help="Copy both binaries to the host temporary filesystem before timing")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 5 <= args.samples <= 1000 or not 0 <= args.warmups <= 100:
        parser.error("samples must be 5..1000 and warmups 0..100")
    provenance = {}
    for label in ("before", "after"):
        provenance[label] = json.loads(getattr(args, label + "_provenance").read_text(encoding="utf-8"))
        actual = hashlib.sha256(getattr(args, label).read_bytes()).hexdigest()
        if provenance[label].get("binary_sha256") != actual:
            parser.error(f"{label} binary does not match its build provenance")
    report = compare(args.before, args.after, args.samples, args.warmups, args.stage_binaries)
    report["builds"] = provenance
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    for name, result in report["latency"].items():
        print(f"{name}: {result['before']['p50']:.3f} -> {result['after']['p50']:.3f} ms median "
              f"({result['median_reduction_percent']:.1f}% reduction)")


if __name__ == "__main__":
    main()
