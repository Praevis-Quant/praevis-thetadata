"""Offline checks of the P1 exit runners: capture reading, conversion-only
measurement, report writing and the live-check parameters (no network)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import praevis_thetadata as pt  # noqa: E402
from praevis_thetadata import _native  # noqa: E402

import common  # noqa: E402
import compare  # noqa: E402
import live_checks  # noqa: E402
import options_memory  # noqa: E402
import options_ours  # noqa: E402


def write_capture(path: Path, frames: list[bytes]) -> None:
    path.write_bytes(b"".join(len(f).to_bytes(4, "big") + f for f in frames))


class Runners(unittest.TestCase):
    def test_captures_round_trip_and_truncation_is_refused(self):
        frames = _native._synthetic_frames(5)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "W1-0.bin"
            write_capture(path, frames)
            self.assertEqual(common.read_capture(path), [bytes(f) for f in frames])
            path.write_bytes(path.read_bytes()[:-1])
            with self.assertRaises(ValueError):
                common.read_capture(path)

    def test_conversion_only_runs_offline_and_writes_a_report(self):
        with tempfile.TemporaryDirectory() as directory:
            captures, out = Path(directory) / "captures", Path(directory) / "out"
            captures.mkdir()
            write_capture(captures / "W1-0.bin", _native._synthetic_frames(252))
            write_capture(captures / "W2-0.bin", _native._synthetic_frames(40, columns=20))
            argv = ["compare.py", "--run-id", "t1", "--skip-network", "--captures", str(captures),
                    "--convert-repeats", "2", "--out", str(out)]
            with mock.patch.object(sys, "argv", argv):
                self.assertEqual(compare.main(), 0)
            report = json.loads((out / "t1.json").read_text())
            rows = {c["file"]: (c["polars"]["rows"], c["pandas"]["rows"], c["decode"]["rows"]) for c in report["convert"]}
            self.assertEqual(rows, {"W1-0.bin": (252, 252, 252), "W2-0.bin": (40, 40, 40)})
            self.assertEqual(report["results"], [])
            # Measured on every supported platform (Windows CI checks the ctypes path).
            self.assertGreater(report["peak_rss_bytes"], 1 << 20)
            # Run ids are never overwritten.
            with mock.patch.object(sys, "argv", argv):
                self.assertEqual(compare.main(), 2)

    def test_reports_with_a_credential_are_refused(self):
        with tempfile.TemporaryDirectory() as directory, \
                mock.patch.dict("os.environ", {"THETADATA_API_KEY": "synthetic-key-123"}):
            path = Path(directory) / "r.json"
            self.assertEqual(common.write_report({"x": "synthetic-key-123"}, path), 3)
            self.assertFalse(path.exists())
            self.assertEqual(common.write_report({"x": 1}, path), 0)

    def test_live_check_parameters_build_valid_requests(self):
        self.assertEqual(len(live_checks.CHECKS), 10)
        for method, params in live_checks.CHECKS:
            with self.subTest(method):
                query, parts = pt.describe(method, **params)
                self.assertEqual(parts, 1)
                self.assertTrue(query)


    def test_every_options_workload_request_builds_one_query(self):
        workloads = json.loads(common.WORKLOADS.read_text())
        ids = [w for w in workloads if w[:2] in {"W4", "W5", "W6"} or w[0] in {"S", "M"}]
        self.assertIn("W5c", ids)
        self.assertIn("M3", ids)
        for workload in ids:
            for request in workloads[workload]["requests"]:
                with self.subTest(workload=workload, request=request):
                    query, parts = pt.describe(workloads[workload]["method"], **common.option_kwargs(request))
                    self.assertEqual(parts, 1)
                    self.assertTrue(query)

    def test_workloads_are_timed_whole_in_order_or_threaded(self):
        import threading
        import time as clock

        class Frame:
            shape = (3, 2)

        in_flight, peak, lock = [0], [0], threading.Lock()

        def call(kwargs):
            with lock:
                in_flight[0] += 1
                peak[0] = max(peak[0], in_flight[0])
            clock.sleep(0.05)
            with lock:
                in_flight[0] -= 1
            if kwargs["symbol"] == "BAD":
                raise RuntimeError("synthetic")
            return Frame()

        requests = [{"symbol": "SPY", "date": "2024-06-14"}] * 7 + [{"symbol": "BAD", "date": "2024-06-14"}]
        for threads, expected_peak in ((1, 1), (8, 8)):
            peak[0] = 0
            result = common.time_workload(call, requests, threads, lambda e: "SYNTHETIC")
            self.assertEqual(peak[0], expected_peak)
            self.assertEqual((result["threads"], result["rows"]), (threads, 21))
            self.assertEqual([r["request"] for r in result["requests"]], list(range(8)))
            self.assertEqual(result["requests"][-1]["status"], "SYNTHETIC")
            self.assertGreaterEqual(result["wall_ns"], max(r["start_ns"] + r["ns"] for r in result["requests"]))
        self.assertTrue(callable(options_ours.main))


    def test_the_memory_runner_records_one_mode_and_removes_the_file(self):
        class Frame:
            shape = (5, 3)

            def estimated_size(self):
                return 1234

        class Stub:
            tier, scheduler = "PRO", (8, None)

            def to_parquet(self, method, path, **kwargs):
                Path(path).write_bytes(b"PAR1")
                return 5, 1, 4

            def option_history_quote(self, **kwargs):
                return Frame()

        with tempfile.TemporaryDirectory() as directory, \
                mock.patch.object(pt.Client, "from_env", return_value=Stub()):
            out = Path(directory)
            for mode, run in (("parquet", "m-parquet"), ("frame", "m-frame")):
                argv = ["options_memory.py", "--run-id", run, "--workload", "M1", "--mode", mode,
                        "--out", str(out)]
                with mock.patch.object(sys, "argv", argv):
                    self.assertEqual(options_memory.main(), 0)
            parquet = json.loads((out / "m-parquet.json").read_text())
            frame = json.loads((out / "m-frame.json").read_text())
            self.assertEqual((parquet["status"], parquet["rows"], parquet["file_bytes"]), ("OK", 5, 4))
            self.assertEqual((frame["status"], frame["rows"], frame["frame_bytes"]), ("OK", 5, 1234))
            self.assertFalse((out / "m-parquet.parquet").exists())
            self.assertGreater(parquet["peak_rss_bytes"], 0)
            # A workload with several requests is refused.
            argv = ["options_memory.py", "--run-id", "x", "--workload", "W5c", "--mode", "frame",
                    "--out", str(out)]
            with mock.patch.object(sys, "argv", argv):
                self.assertEqual(options_memory.main(), 2)


if __name__ == "__main__":
    unittest.main()
