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
  request: its running parts are cancelled and no more are sent.
  Cancellation is local: it does not prove that the service stopped its work
  at the same instant.

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


def cancel_midway(client: pt.Client, settle: float = 1.5) -> int:
    """Read the first batch of a long planned job (six yearly parts, two at a
    time), then cancel it. Returns the rows read before cancelling. ``settle``
    waits for as long as the running parts would take, so a job that was not
    cancelled would have started its next parts by then."""
    job = client.batches(
        "stock_history_eod", symbol="SYNTHETIC", start_date="2020-01-01", end_date="2025-12-31", concurrency=2
    )
    first = len(next(job))
    job.cancel()  # the cancellation; dropping the iterator does the same
    time.sleep(settle)
    return first


class Loopback:
    """A running loopback service and a client of it."""

    def __init__(self, process: subprocess.Popen, client: pt.Client) -> None:
        self._process = process
        self.client = client

    def requests(self) -> int:
        """The stock EOD requests the service has received so far."""
        self._process.stdin.write("requests\n")
        self._process.stdin.flush()
        return json.loads(self._process.stdout.readline())["requests"]


@contextlib.contextmanager
def loopback(scenario: str, server: str | None = None, **options) -> Iterator[Loopback]:
    """A fresh loopback service running ``scenario``, and a client of it
    (``options`` go to the client)."""
    server = server or os.environ["PRAEVIS_LOOPBACK_SERVER"]
    process = subprocess.Popen([server, scenario], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    try:
        urls = json.loads(process.stdout.readline())
        client = pt.Client(
            api_key="synthetic", dataframe_type="arrow",
            _auth_url=urls["auth_url"], _endpoint=urls["endpoint"], **options,
        )
        yield Loopback(process, client)
    finally:
        process.stdin.close()  # the server stops when its input closes
        process.wait(timeout=30)
        process.stdout.close()


def main() -> None:
    with loopback("partial") as service:
        client = service.client
        outcome, rows, kind = keep_partial(client)
        print(f"{outcome}: kept {rows} rows delivered before the failure ({kind}); not complete")
        try:
            client.query("stock_history_eod", symbol="SYNTHETIC", start_date="2026-01-15", end_date="2026-01-15")
        except pt.QueryFailed as error:
            print(f"query() is all-or-error: it raised {error.args[0]!r} and kept no rows")
    with loopback("slow") as service:
        first = cancel_midway(service.client)
        print(f"cancelled a job after its first batch ({first} rows); the service saw {service.requests()} of its 6 parts")


if __name__ == "__main__":
    main()
