"""Missing or ambiguous enhancement provenance must fail the repository audit."""
import copy
import unittest

from check_repository import audit_origins


class OriginAuditTests(unittest.TestCase):
    def setUp(self):
        self.requirements = {"TEST-L1-001", "TEST-L2-001"}
        self.adrs = {"docs/adr/0001-test.md"}
        self.paths = self.adrs | {"docs/requirements/test.md"}
        self.registry = {
            "schema_version": 1,
            "groups": [{"id": "test", "origin": "mixed",
                        "requirements": sorted(self.requirements),
                        "rationale": "Wire values plus exact Rust representation.",
                        "protected_contract": "Keep exact representation.",
                        "evidence": ["docs/requirements/test.md"]}],
            "adrs": [{"path": "docs/adr/0001-test.md", "origin": "mixed",
                      "rationale": "Observed wire plus local representation."}],
        }

    def errors(self):
        return audit_origins(self.registry, self.requirements, self.adrs, self.paths)

    def test_complete_register(self):
        self.assertEqual(self.errors(), [])

    def test_new_requirement_or_adr_cannot_bypass_classification(self):
        self.requirements.add("TEST-L3-001")
        self.adrs.add("docs/adr/0002-new.md")
        self.assertTrue(any("Unclassified requirement" in e for e in self.errors()))
        self.assertTrue(any("Unclassified ADR" in e for e in self.errors()))

    def test_duplicate_group_and_requirement(self):
        self.registry["groups"].append(copy.deepcopy(self.registry["groups"][0]))
        self.assertTrue(any("duplicate origin group" in e for e in self.errors()))
        self.assertTrue(any("Duplicate origin classification" in e for e in self.errors()))

    def test_unknown_and_removed_contracts(self):
        self.requirements.remove("TEST-L2-001")
        self.adrs.clear()
        self.assertTrue(any("Unknown classified requirement" in e for e in self.errors()))
        self.assertTrue(any("Unknown classified ADR" in e for e in self.errors()))

    def test_invalid_origin_and_missing_protected_policy(self):
        self.registry["groups"][0]["origin"] = "supported"
        self.registry["groups"][0]["protected_contract"] = " "
        self.assertTrue(any("Invalid origin" in e for e in self.errors()))
        self.assertTrue(any("Missing protected_contract" in e for e in self.errors()))

    def test_missing_or_untracked_evidence(self):
        self.registry["groups"][0]["evidence"] = ["missing.md"]
        self.assertTrue(any("Untracked/missing origin evidence" in e for e in self.errors()))
        self.registry["groups"][0]["evidence"] = []
        self.assertTrue(any("Missing requirements/evidence" in e for e in self.errors()))

    def test_invalid_duplicate_adr_and_missing_rationale(self):
        self.registry["adrs"][0].update(origin="planned", rationale="")
        self.registry["adrs"].append(copy.deepcopy(self.registry["adrs"][0]))
        self.assertTrue(any("Invalid ADR origin" in e for e in self.errors()))
        self.assertTrue(any("Missing ADR origin rationale" in e for e in self.errors()))
        self.assertTrue(any("Duplicate ADR origin" in e for e in self.errors()))

    def test_schema_version(self):
        self.registry["schema_version"] = 99
        self.assertTrue(any("schema version" in e for e in self.errors()))


if __name__ == "__main__":
    unittest.main()
