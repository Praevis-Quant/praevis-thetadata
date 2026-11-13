import json
import pathlib
import tempfile
import unittest

from emulator_drift import MODEL, SCHEMAS, STATUS, drift, main

SCHEMA = json.loads(SCHEMAS.read_text())
STATE = json.loads(STATUS.read_text())
MODELED = json.loads(MODEL.read_text())
EOD = [list(c) for c in SCHEMA["GetStockHistoryEod"]["columns"]]


def live(*scenarios):
    return {"run_id": "live-001", "target": "live", "scenarios": list(scenarios)}


def passed(name, observed):
    return {"name": name, "status": "pass", "detail": "", "observed": observed}


def findings(*reports, **options):
    return drift(list(reports), SCHEMA, STATE, MODELED, options.get("profile", "options-pro"),
                 options.get("timing_factor", 5.0))


class DriftTests(unittest.TestCase):
    def test_a_report_that_matches_the_model_has_no_findings(self):
        report = live(
            passed("login-tier", {"tier": "PRO"}),
            passed("query-stock-eod", {"rpc": "GetStockHistoryEod", "code": "OK", "columns": EOD}),
            passed("tier-refusal", {"rpc": "GetStockSnapshotOhlc", "code": "PermissionDenied"}),
            passed("not-found-is-identical", {"rpc": "GetStockHistoryEod", "code": "NotFound"}),
            passed("stream-full-trades", {"first_trade_ms": MODELED["terminal"]["first_event_ms"]}),
        )
        self.assertEqual(findings(report), [])

    def test_every_kind_of_difference_is_a_finding(self):
        changed = [list(c) for c in EOD]
        changed[0][1] = "number" if changed[0][1] != "number" else "text"
        report = live(
            passed("login-tier", {"tier": "STANDARD"}),
            passed("query-stock-eod", {"rpc": "GetStockHistoryEod", "code": "OK", "columns": changed}),
            passed("tier-refusal", {"rpc": "GetStockSnapshotOhlc", "code": "OK"}),
            passed("stream-full-trades", {"first_trade_ms": 60_000}),
            {"name": "cancel-midway", "status": "fail", "detail": "QueryFailed: the service's own words",
             "observed": None, "error": {"kind": "remote", "code": "PermissionDenied"}},
        )
        found = findings(report)
        # A failure is recorded by kind and code only, never its detail text.
        failed = next(f for f in found if f["what"] == "scenario failed")
        self.assertEqual(failed["observed"], {"kind": "remote", "code": "PermissionDenied"})
        self.assertNotIn("own words", json.dumps(found))
        whats = sorted(f["what"] for f in found)
        self.assertEqual(whats, sorted([
            "account tier", "GetStockHistoryEod: columns", "GetStockSnapshotOhlc: code",
            "first stream event (ms)", "scenario failed",
        ]))

    def test_an_rpc_never_probed_or_without_a_schema_is_named(self):
        unprobed = next(rpc for rpc in STATE if "pro" not in STATE[rpc])
        report = live(
            passed("x", {"rpc": unprobed, "code": "OK"}),
            passed("y", {"rpc": "GetNoSuchRpc", "code": "OK", "columns": []}),
        )
        whats = [f["what"] for f in findings(report)]
        self.assertIn(f"{unprobed}: never probed on the options-pro profile", whats)
        self.assertIn("GetNoSuchRpc: no registered schema", whats)

    def test_an_emulator_report_is_not_evidence(self):
        report = {"run_id": "ci", "target": "emulator", "scenarios": []}
        self.assertEqual([f["what"] for f in findings(report)], ["not a live report"])

    def test_the_model_files_are_never_written(self):
        before = [path.read_bytes() for path in (SCHEMAS, STATUS, MODEL)]
        with tempfile.TemporaryDirectory() as directory:
            report = pathlib.Path(directory) / "r.json"
            report.write_text(json.dumps(live(passed("login-tier", {"tier": "FREE"}))))
            out = pathlib.Path(directory) / "drift.json"
            self.assertEqual(main([str(report), "--out", str(out)]), 0)
            self.assertEqual(main([str(report), "--out", str(out), "--strict"]), 1)
            self.assertEqual(len(json.loads(out.read_text())["findings"]), 1)
        self.assertEqual([path.read_bytes() for path in (SCHEMAS, STATUS, MODEL)], before)


if __name__ == "__main__":
    unittest.main()
