"""Arrow hand-off through the PyCapsule interface (no network)."""
import unittest

import pyarrow as pa

import praevis_thetadata as pt
from praevis_thetadata import _native


class ArrowTests(unittest.TestCase):
    def setUp(self):
        self.result = pt.Result(_native._synthetic(10_000, columns=16, per_message=1000))

    def test_exact_types_reach_pyarrow(self):
        table = pa.table(self.result)
        self.assertEqual(table.num_rows, 10_000)
        self.assertEqual(table.schema.field("c2").type, pa.decimal128(38, 10))
        self.assertEqual(table.schema.field("c0").type, pa.timestamp("ms", tz="America/New_York"))
        self.assertEqual(table.schema.field("c8").type, pa.int64())
        # 18_900 at wire scale 8 is 189.00.
        self.assertEqual(str(table.column("c2")[0].as_py()), "189.0000000000")

    def test_each_export_is_an_independent_stream(self):
        first, second = pa.table(self.result), pa.table(self.result)
        self.assertTrue(first.equals(second))
        self.assertEqual(len(self.result), 10_000)

    def test_batches_export_as_record_batches(self):
        batches = self.result.batches()
        self.assertGreaterEqual(len(batches), 1)
        rebuilt = pa.Table.from_batches([pa.record_batch(b) for b in batches])
        self.assertTrue(rebuilt.equals(pa.table(self.result)))

    def test_polars_and_pandas_keep_exact_types(self):
        import polars as pl

        frame = self.result.to_polars()
        self.assertEqual(frame.height, 10_000)
        self.assertEqual(frame.schema["c2"], pl.Decimal(38, 10))
        pandas_frame = self.result.to_pandas()
        self.assertTrue(str(pandas_frame["c2"].dtype).startswith("decimal128(38, 10)"))

    def test_empty_result(self):
        empty = pt.Result(_native._synthetic(0))
        self.assertEqual(len(empty), 0)
        self.assertEqual(pa.table(empty).num_rows, 0)


if __name__ == "__main__":
    unittest.main()
