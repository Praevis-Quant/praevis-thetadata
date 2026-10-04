"""Measure synthetic auth/save and persisted-status CLI latency; never use live auth."""
import argparse
from datetime import datetime, timezone
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import threading
import time
import uuid

TOKEN = "synthetic-benchmark-session"
KEY = "synthetic-benchmark-api-key"


class IdentityFixture(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if self.path != "/auth" or body != {"apiKey": KEY, "authEnv": {"envType": "STAGE"}}:
            self.send_error(400)
            return
        payload = json.dumps({"sessionId": TOKEN, "user": {"email": "benchmark@example.invalid", "stockSubscription": "SYNTHETIC"}}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def summarize(values):
    ordered = sorted(values)
    return {"samples": len(values), "min": min(values), "p50": statistics.median(values),
            "p95": ordered[math.ceil(0.95 * len(ordered)) - 1], "max": max(values), "mean": statistics.mean(values)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=30)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.samples < 5 or args.samples > 1000:
        parser.error("--samples must be between 5 and 1000")
    binary = args.binary.resolve(strict=True)
    environment = {k: v for k, v in os.environ.items() if not k.startswith("THETADATA_")}
    profile = "bench-" + uuid.uuid4().hex
    base = [str(binary), "auth", "--profile", profile, "--environment", "stage"]

    def invoke(arguments, login=False):
        child_env = environment.copy()
        if login:
            child_env["THETADATA_API_KEY"] = KEY
        start = time.perf_counter_ns()
        result = subprocess.run(base + arguments, env=child_env, capture_output=True, timeout=40, check=False)
        elapsed_ms = (time.perf_counter_ns() - start) / 1_000_000
        if TOKEN.encode() in result.stdout + result.stderr or KEY.encode() in result.stdout + result.stderr:
            raise RuntimeError("CLI exposed a synthetic secret; output withheld")
        if result.returncode:
            raise RuntimeError(f"CLI failed with exit {result.returncode}: {result.stderr.decode(errors='replace').strip()}")
        return elapsed_ms, json.loads(result.stdout)

    server = ThreadingHTTPServer(("127.0.0.1", 0), IdentityFixture)
    thread = threading.Thread(target=lambda: server.serve_forever(poll_interval=0.05), daemon=True)
    thread.start()
    auth_args = ["--auth-url", f"http://127.0.0.1:{server.server_port}/auth", "--insecure", "--json"]
    timings = {"auth_and_save_ms": [], "persisted_status_ms": []}
    try:
        for index in range(args.samples + 2):
            elapsed, data = invoke(auth_args, login=True)
            if not data.get("authenticated") or not data.get("persisted"):
                raise RuntimeError("Authentication did not save a session")
            if index >= 2:
                timings["auth_and_save_ms"].append(elapsed)
        # Stop the fixture before status: persistence must not depend on a live server.
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()
        for index in range(args.samples + 2):
            elapsed, data = invoke(["status", "--json"])
            if not data.get("persisted") or data.get("validity") != "not_checked":
                raise RuntimeError("Status did not reload the saved session")
            if index >= 2:
                timings["persisted_status_ms"].append(elapsed)
        head = subprocess.run(["git", "rev-parse", "--verify", "HEAD"], capture_output=True, text=True, check=False)
        revision = head.stdout.strip() if head.returncode == 0 else None
        dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], text=True).strip())
        report = {"schema_version": 1, "measured_at_utc": datetime.now(timezone.utc).isoformat(),
            "git_revision": revision, "worktree_dirty": dirty, "platform": platform.platform(), "python": platform.python_version(),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "storage": data["storage"],
            "includes_process_startup": True, "transport": "localhost HTTP fixture; no TLS", "live_vendor_calls": False,
            "warmup_iterations_per_case": 2, "latency": {name: summarize(values) for name, values in timings.items()}}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(report, indent=2))
    finally:
        server.shutdown()
        server.server_close()
        invoke(["logout", "--json"])


if __name__ == "__main__":
    main()
