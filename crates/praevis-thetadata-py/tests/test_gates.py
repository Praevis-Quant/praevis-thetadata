"""Plan D.2 gates (no network).

1. No per-row Python objects: Python allocations while producing and
   converting rows do not grow with the row count.
2. Pass-through overhead: the Python layer, and a vendor-named alias, add at
   most a fixed few microseconds per call over the native call.

PRAEVIS_GATE_ROWS=1000000,10000000 runs the plan's full sizes manually; CI uses
smaller ones.
"""
import os
import statistics
import sys
import time
import tracemalloc
import unittest

import praevis_thetadata as pt
from praevis_thetadata import _native

ROWS = [int(n) for n in os.environ.get("PRAEVIS_GATE_ROWS", "100000,1000000").split(",")]


def python_allocations(rows: int, convert) -> tuple[int, int]:
    """(net allocated blocks, tracemalloc peak bytes) while producing and
    converting `rows` rows. Arrow buffers live in Rust/Arrow memory."""
    result = pt.Result(_native._synthetic(1000))  # warm imports and caches
    convert(result)
    del result
    tracemalloc.start()
    before = sys.getallocatedblocks()
    result = pt.Result(_native._synthetic(rows))
    converted = convert(result)
    after = sys.getallocatedblocks()
    _, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    del converted, result
    return after - before, peak


class AllocationGate(unittest.TestCase):
    def check(self, convert):
        small, large = ROWS[0], ROWS[-1]
        blocks_small, peak_small = python_allocations(small, convert)
        blocks_large, peak_large = python_allocations(large, convert)
        print(f"\n{convert.__name__}: blocks {blocks_small} at {small:,} rows, {blocks_large} at {large:,}; "
              f"tracemalloc peak {peak_small} / {peak_large} bytes", file=sys.stderr)
        # Constant, not per row: the larger run adds far fewer than one block
        # per 100 extra rows, and stays within a small absolute bound.
        self.assertLess(blocks_large - blocks_small, (large - small) // 100)
        self.assertLess(abs(blocks_large), 5_000)
        self.assertLess(peak_large, 2_000_000)

    def test_arrow(self):
        def to_arrow(result):
            return result.to_arrow()
        self.check(to_arrow)

    def test_polars(self):
        def to_polars(result):
            return result.to_polars()
        self.check(to_polars)


class OverheadGate(unittest.TestCase):
    def test_python_layer_adds_a_fixed_few_microseconds(self):
        kwargs = dict(symbol="AAPL", start_date="2024-01-02", end_date="2024-01-05")
        calls = 2000

        def per_call(function) -> float:
            samples = []
            for _ in range(7):
                start = time.perf_counter()
                for _ in range(calls):
                    function("stock_history_eod", **kwargs)
                samples.append((time.perf_counter() - start) / calls * 1e6)
            return statistics.median(samples)

        native = per_call(_native.describe)
        wrapped = per_call(pt.describe)
        print(f"\nnative {native:.2f} µs/call, Python layer {wrapped:.2f} µs/call", file=sys.stderr)
        self.assertLess(wrapped - native, 5.0)


class AliasOverheadGate(unittest.TestCase):
    """D.2 gate: a vendor-named alias adds at most a fixed few microseconds.

    Timing noise on a shared runner only ever adds time. So the two paths are
    timed in interleaved blocks (alternating which goes first), and each
    side's **minimum** block is compared. Timing all of one path and then all
    of the other let noise between the two phases decide the result: Windows
    CI saw -12.6 to +9.3 µs on unchanged code."""

    LIMIT_US = 5.0

    def setUp(self):
        import pyarrow as pa

        table = _native._synthetic(3)

        class Native:
            def query(self, name, *, plan, **params):
                return table

        native = Native()
        client = object.__new__(pt.Client)
        object.__setattr__(client, "_native", native)
        object.__setattr__(client, "_dataframe_type", "arrow")
        start, end = "2024-01-02", "2024-01-05"
        self.direct = lambda: pa.table(pt.Result(native.query(
            "stock_history_eod", plan=True, symbol="AAPL", start_date=start, end_date=end)))
        self.alias = lambda: client.stock_history_eod("AAPL", start, end)

    @staticmethod
    def overhead(direct, alias, blocks: int = 21, calls: int = 500) -> tuple[float, float]:
        """(direct µs per call, alias overhead µs per call), minimum per side
        over interleaved blocks."""
        def block(function) -> float:
            begin = time.perf_counter()
            for _ in range(calls):
                function()
            return (time.perf_counter() - begin) / calls * 1e6

        block(direct), block(alias)  # warm
        bases, wrapped = [], []
        for index in range(blocks):
            if index % 2:
                wrapped.append(block(alias))
                bases.append(block(direct))
            else:
                bases.append(block(direct))
                wrapped.append(block(alias))
        return min(bases), min(wrapped) - min(bases)

    def test_a_vendor_alias_adds_a_fixed_few_microseconds(self):
        base, added = self.overhead(self.direct, self.alias)
        print(f"\nnative call + conversion {base:.2f} µs, vendor alias adds {added:.2f} µs "
              f"(minimum of 21 interleaved blocks per side)", file=sys.stderr)
        self.assertLess(added, self.LIMIT_US)

    def test_the_gate_still_fails_a_genuinely_slow_alias(self):
        # Mutation check: an alias that spends at least 10 µs more per call
        # must fail the same measurement, so the minimum cannot hide real cost.
        def slow_alias():
            stop = time.perf_counter() + 10e-6
            while time.perf_counter() < stop:
                pass
            return self.alias()

        _, added = self.overhead(self.direct, slow_alias, blocks=7, calls=200)
        self.assertGreaterEqual(added, self.LIMIT_US)

if __name__ == "__main__":
    unittest.main()
