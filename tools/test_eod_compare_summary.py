"""Offline checks for the comparison summary; synthetic reports only."""
import json
from pathlib import Path
import tempfile
import unittest

import eod_compare_summary as summary


def report(directory, name, baseline_ns, candidate_ns, engines=("Numeric", "Numeric"), extra=None):
    def stats(ns):
        return {metric: {"p50_ns": ns, "p95_ns": ns} for metric in summary.METRICS}
    path = Path(directory) / name
    path.write_text(json.dumps({"baseline_engine": engines[0], "candidate_engine": engines[1],
                                "cases": [{"case": "nulls-none", "baseline_summary": stats(baseline_ns),
                                           "candidate_summary": stats(candidate_ns)}],
                                **(extra or {})}))
    return path


class CompareSummaryTests(unittest.TestCase):
    def test_ratios_per_round_and_markdown_range(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = [report(directory, "r1.json", 100, 110), report(directory, "r2.json", 100, 90)]
            data = summary.ratios(paths)
            self.assertEqual(data[("Numeric", "nulls-none")]["decode"], [1.1, 0.9])
            self.assertIn("| Numeric | nulls-none | 2 | 0.900-1.100 |", summary.markdown(data))

    def test_dependency_ab_is_labelled_with_both_lockfiles(self):
        dependency = {"comparison_kind": "dependency A/B",
                      "baseline_lock_sha256": "a" * 64, "candidate_lock_sha256": "b" * 64}
        with tempfile.TemporaryDirectory() as directory:
            paths = [report(directory, f"r{n}.json", 100, 101, extra=dependency) for n in (1, 2)]
            label, header = summary.kind(paths)
            self.assertEqual(label, "dependency A/B")
            self.assertIn("**Dependency A/B**", header)
            self.assertIn("aaaaaaaaaaaa", header)
            self.assertIn("bbbbbbbbbbbb", header)
            # Plain reports, including ones that predate the field, get no header.
            plain = [report(directory, "p.json", 100, 100)]
            self.assertEqual(summary.kind(plain), ("A/B", ""))
            with self.assertRaisesRegex(ValueError, "mix"):
                summary.kind(paths + plain)

    def test_mismatched_engines_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = report(directory, "r.json", 100, 100, ("Numeric", "Table"))
            with self.assertRaisesRegex(ValueError, "engines differ"):
                summary.ratios([path])


if __name__ == "__main__":
    unittest.main()
