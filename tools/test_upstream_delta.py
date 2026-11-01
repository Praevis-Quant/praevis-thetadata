"""Offline checks for the static upstream wheel comparison; no vendor code or network."""
import base64
import hashlib
from pathlib import Path
import tempfile
import unittest
import zipfile

from research_python import sha
import upstream_delta

OLD = '''class ThetaClient:
    def stock_snapshot_quote(self, symbol: str, min_time=None):
        query = Q()
        if min_time:
            query.min_time = min_time
        return self.stub.GetStockSnapshotQuote(query)
'''
NEW = OLD.replace("min_time=None):", "min_time=None, current_day_only: bool=False):").replace(
    "        return", "        if current_day_only:\n            query.current_day_only = current_day_only\n        return")


def wheel(directory, version, client, corrupt=False):
    files = {"thetadata/client.py": client.encode(),
             f"thetadata-{version}.dist-info/METADATA": f"Metadata-Version: 2.4\nName: thetadata\nVersion: {version}\n\n".encode()}
    record = []
    for name, data in files.items():
        digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).decode().rstrip("=")
        record.append(f"{name},sha256={digest},{len(data) + int(corrupt)}")
    record_name = f"thetadata-{version}.dist-info/RECORD"
    files[record_name] = ("\n".join(record + [f"{record_name},,"]) + "\n").encode()
    path = Path(directory) / f"thetadata-{version}-py3-none-any.whl"
    with zipfile.ZipFile(path, "w") as archive:
        for name, data in files.items():
            archive.writestr(name, data)
    return path, sha(path.read_bytes())


class UpstreamDeltaTests(unittest.TestCase):
    def test_detects_added_parameter_and_truthiness_guarded_binding(self):
        with tempfile.TemporaryDirectory() as directory:
            old = upstream_delta.load(*wheel(directory, "1.0.0", OLD))
            new = upstream_delta.load(*wheel(directory, "1.0.1", NEW))
            result = upstream_delta.delta(old, new)
        change = result["wrappers_changed"]["ThetaClient.stock_snapshot_quote"]
        self.assertEqual([p["name"] for p in change["parameters_added"]], ["current_day_only"])
        self.assertEqual(change["parameters_added"][0]["default"], "False")
        self.assertEqual(change["bindings_added"], [{"target": "query.current_day_only",
                                                     "expression": "current_day_only", "when": ["current_day_only"]}])
        self.assertEqual(result["metadata_changes"], {"Version": {"old": ["1.0.0"], "new": ["1.0.1"]}})
        self.assertEqual(result["rpcs"], {"added": {}, "removed": [], "changed": []})

    def test_rejects_unexpected_digest_and_invalid_record(self):
        with tempfile.TemporaryDirectory() as directory:
            path, _ = wheel(directory, "1.0.0", OLD)
            with self.assertRaisesRegex(ValueError, "expected PyPI digest"):
                upstream_delta.load(path, "0" * 64)
            path, digest = wheel(directory, "1.0.2", OLD, corrupt=True)
            with self.assertRaisesRegex(ValueError, "RECORD entry"):
                upstream_delta.load(path, digest)


if __name__ == "__main__":
    unittest.main()
