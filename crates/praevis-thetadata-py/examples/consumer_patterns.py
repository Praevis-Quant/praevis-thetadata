"""Runnable consumer patterns: partial results and cancellation.

It runs against the synthetic loopback service (no credentials, no ThetaData
traffic), started from the Rust example ``loopback_server``:

    cargo build -p praevis-thetadata-client --example loopback_server
    PRAEVIS_LOOPBACK_SERVER=target/debug/examples/loopback_server \\
        python crates/praevis-thetadata-py/examples/consumer_patterns.py

It shows how to:

- **keep partial results:** iterate ``batches()`` and keep what arrived when a
  later batch fails. ``query()`` is all-or-error: it raises and keeps nothing.
  Call the rows complete only when the iteration ends without an error.
- **cancel mid-way:** ``cancel()`` the iterator (or drop it) to stop the
  request. Its scheduler slot is released at once, so the next request starts
  without waiting. Cancellation is local: it does not prove that the service
  stopped its work at the same instant.

The ``_auth_url`` and ``_endpoint`` keywords are a test seam for this loopback
service only (plain HTTP on 127.0.0.1); real code passes credentials.
"""
from __future__ import annotations

import contextlib
import json
import os
import subprocess
import time
from typing import Iterator

import praevis_thetadata as pt


def keep_partial(client: pt.Client) -> tuple[str, int, str | None]:
    """``("complete", rows, None)`` or ``("partial", rows, error_kind)``."""
    rows = 0
    try:
        for batch in client.batches(
            "stock_history_eod", symbol="SYNTHETIC", start_date="2026-01-15", end_date="2026-01-15"
        ):
            rows += len(batch)  # process the batch here
    except pt.QueryFailed as error:
        kind = error.args[0]  # for example "remote"; error.args[2] is the gRPC code
        return "partial", rows, kind
    return "complete", rows, None


def cancel_midway(client: pt.Client) -> tuple[int, float]:
    """Read the first batch of a long job, cancel it, then time how long the
    next request waits for its first batch."""
    job = client.batches("stock_history_eod", symbol="SYNTHETIC", start_date="2020-01-01", end_date="2025-12-31")
    first = len(next(job))
    job.cancel()  # the cancellation; dropping the iterator does the same
    started = time.monotonic()
    # A timeout includes time queued for a slot: if the job still held it,
    # this would fail after 5 s instead of waiting.
    following = client.batches(
        "stock_history_eod", symbol="SYNTHETIC", start_date="2026-01-15", end_date="2026-01-15", timeout=5.0
    )
    next(following)
    waited = time.monotonic() - started
    following.cancel()
    return first, waited


@contextlib.contextmanager
def loopback(scenario: str, server: str | None = None) -> Iterator[pt.Client]:
    """A client of a fresh loopback service running ``scenario``. One request
    at a time, so a request that was not cancelled would block the next."""
    server = server or os.environ["PRAEVIS_LOOPBACK_SERVER"]
    process = subprocess.Popen([server, scenario], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    try:
        urls = json.loads(process.stdout.readline())
        yield pt.Client(
            api_key="synthetic", concurrency=1, per_minute=0, dataframe_type="arrow",
            _auth_url=urls["auth_url"], _endpoint=urls["endpoint"],
        )
    finally:
        process.stdin.close()  # the server stops when its input closes
        process.wait(timeout=30)
        process.stdout.close()


def main() -> None:
    with loopback("partial") as client:
        outcome, rows, kind = keep_partial(client)
        print(f"{outcome}: kept {rows} rows delivered before the failure ({kind}); not complete")
        try:
            client.query("stock_history_eod", symbol="SYNTHETIC", start_date="2026-01-15", end_date="2026-01-15")
        except pt.QueryFailed as error:
            print(f"query() is all-or-error: it raised {error.args[0]!r} and kept no rows")
    with loopback("slow") as client:
        first, waited = cancel_midway(client)
        print(f"cancelled a job after its first batch ({first} rows); the next request's first batch came {waited * 1000:.0f} ms later")


if __name__ == "__main__":
    main()
