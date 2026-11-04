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


if __name__ == "__main__":
    unittest.main()
