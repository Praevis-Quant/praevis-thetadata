"""Shared pieces of the P1 exit runners: pacing, peak memory, capture files
and the identity check. Timings and counts only; never a row value or a key."""
from __future__ import annotations

import ctypes
import json
import os
import platform
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKLOADS = ROOT / "bench" / "python-baseline" / "workloads.json"


class Pacer:
    """At least `interval` seconds between network requests (FREE: 20/min documented)."""

    def __init__(self, interval: float) -> None:
        self.interval = interval
        self.last = None

    def wait(self) -> None:
        if self.last is not None:
            delay = self.last + self.interval - time.monotonic()
            if delay > 0:
                time.sleep(delay)
        self.last = time.monotonic()


def peak_rss_bytes() -> int | None:
    """Peak resident memory of this process so far (indicative)."""
    if sys.platform == "win32":
        from ctypes import wintypes

        class Counters(ctypes.Structure):
            _fields_ = [("cb", ctypes.c_ulong), ("PageFaultCount", ctypes.c_ulong),
                        ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
                        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
                        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t), ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                        ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t)]
        # Declared types matter: the pseudo-handle from GetCurrentProcess is a
        # 64-bit HANDLE (-1); passed as a default 32-bit int it is invalid.
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        psapi = ctypes.WinDLL("psapi", use_last_error=True)
        kernel32.GetCurrentProcess.restype = wintypes.HANDLE
        kernel32.GetCurrentProcess.argtypes = []
        psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
        psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
        counters = Counters()
        counters.cb = ctypes.sizeof(Counters)
        if psapi.GetProcessMemoryInfo(kernel32.GetCurrentProcess(), ctypes.byref(counters), counters.cb):
            return int(counters.PeakWorkingSetSize)
        return None
    import resource

    peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return int(peak) if sys.platform == "darwin" else int(peak) * 1024


def read_capture(path: Path) -> list[bytes]:
    """A baseline capture: 4-byte big-endian length-prefixed ResponseData."""
    data = path.read_bytes()
    frames, at = [], 0
    while at < len(data):
        length = int.from_bytes(data[at:at + 4], "big")
        frames.append(data[at + 4:at + 4 + length])
        at += 4 + length
    if at != len(data):
        raise ValueError(f"{path.name}: truncated capture")
    return frames


def host() -> dict:
    return {"platform": platform.platform(), "machine": platform.machine(),
            "processor": platform.processor(), "python": sys.version.split()[0]}


def write_report(report: dict, path: Path) -> int:
    """Refuse to write if the API key or a credentials-file line appears."""
    text = json.dumps(report, indent=2)
    secrets = [os.environ.get("THETADATA_API_KEY", "").strip(),
               os.environ.get("THETADATA_EMAIL", "").strip(),
               os.environ.get("THETADATA_PASSWORD", "")]
    if any(secret and secret in text for secret in secrets):
        print("Refusing to write the report: it would contain a credential.", file=sys.stderr)
        return 3
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        print(f"{path} exists; choose another --run-id", file=sys.stderr)
        return 2
    path.write_text(text)
    print(f"Wrote {path}")
    return 0


def option_kwargs(request: dict) -> dict:
    """A workloads.json options request as keyword arguments both packages
    accept: `date`, `start_date` and `end_date` as `datetime.date`; expirations, strikes and times stay
    text (the vendor signature accepts text for those)."""
    import datetime as dt

    kwargs = dict(request)
    for key in ("date", "start_date", "end_date"):
        if key in kwargs:
            kwargs[key] = dt.date.fromisoformat(kwargs[key])
    return kwargs


def time_workload(call, requests: list[dict], threads: int, status_of) -> dict:
    """Run every request of one workload, `threads` at a time (1: in order),
    and time the whole workload. `call(kwargs)` returns a frame; `status_of`
    turns an exception into a status name (or raises to stop). Timings and
    counts only: the frames are dropped at once."""
    from concurrent.futures import ThreadPoolExecutor

    start = time.perf_counter_ns()

    def one(index: int) -> dict:
        begin = time.perf_counter_ns()
        try:
            frame = call(option_kwargs(requests[index]))
        except Exception as error:  # noqa: BLE001 - classified by the caller
            return {"request": index, "status": status_of(error),
                    "start_ns": begin - start, "ns": time.perf_counter_ns() - begin, "rows": 0}
        rows = int(frame.shape[0])
        del frame
        return {"request": index, "status": "OK", "start_ns": begin - start,
                "ns": time.perf_counter_ns() - begin, "rows": rows}

    if threads == 1:
        runs = [one(i) for i in range(len(requests))]
    else:
        with ThreadPoolExecutor(max_workers=threads) as pool:
            runs = list(pool.map(one, range(len(requests))))
    return {"threads": threads, "wall_ns": time.perf_counter_ns() - start,
            "rows": sum(r["rows"] for r in runs), "requests": runs}
