"""Mutations that must not pass the requirement hierarchy audit."""
import unittest

from check_repository import audit_requirements


class RequirementAuditTests(unittest.TestCase):
    def documents(self):
        return {f"docs/requirements/{namespace}.md": (
            f"| {namespace}-L1-001 | Outcome |\n"
            f"| {namespace}-L2-001 | {namespace}-L1-001 | System |\n"
            f"| {namespace}-L3-001 | {namespace}-L2-001 | Component |\n"
        ) for namespace in ("AUTH", "EOD")}

    def test_valid_multiple_namespaces_and_moved_component_definition(self):
        docs = self.documents()
        docs["crates/client/docs/requirements/eod.md"] = docs.pop("docs/requirements/EOD.md")
        defined, errors = audit_requirements(docs)
        self.assertEqual(len(defined), 6)
        self.assertEqual(errors, [])

    def test_duplicate_within_or_across_files(self):
        for same_file in (True, False):
            with self.subTest(same_file=same_file):
                docs = self.documents()
                path = "docs/requirements/EOD.md" if same_file else "docs/requirements/copy.md"
                docs[path] = docs.get(path, "") + "| EOD-L3-001 | EOD-L2-001 | Duplicate |\n"
                self.assertTrue(any("Duplicate" in e for e in audit_requirements(docs)[1]))

    def test_undefined_reference_in_non_requirements_document(self):
        docs = self.documents()
        docs["docs/adr/decision.md"] = "See EOD-L3-999."
        self.assertTrue(any("Undefined requirements" in e for e in audit_requirements(docs)[1]))

    def test_missing_or_invalid_parent(self):
        for parent, expected in (("unassigned", "Missing parents"),
                                 ("EOD-L1-001", "Wrong parent level"),
                                 ("EOD-L3-001", "Wrong parent level"),
                                 ("EOD-L2-999", "Undefined parent")):
            with self.subTest(parent=parent):
                docs = self.documents()
                docs["docs/requirements/EOD.md"] = docs["docs/requirements/EOD.md"].replace(
                    "| EOD-L3-001 | EOD-L2-001 |", f"| EOD-L3-001 | {parent} |")
                self.assertTrue(any(expected in e for e in audit_requirements(docs)[1]))

    def test_second_parent_is_also_validated(self):
        docs = self.documents()
        docs["docs/requirements/EOD.md"] = docs["docs/requirements/EOD.md"].replace(
            "| EOD-L3-001 | EOD-L2-001 |", "| EOD-L3-001 | EOD-L2-001, AUTH-L1-001 |")
        self.assertTrue(any("Wrong parent level" in e for e in audit_requirements(docs)[1]))

    def test_missing_level_in_new_namespace(self):
        docs = self.documents()
        docs["docs/requirements/OTHER.md"] = "| OTHER-L1-001 | Outcome |\n"
        self.assertTrue(any("Missing OTHER L2" in e for e in audit_requirements(docs)[1]))

    def test_definition_outside_canonical_directory(self):
        docs = self.documents()
        docs["docs/notes.md"] = docs.pop("docs/requirements/EOD.md")
        self.assertTrue(any("outside requirements directory" in e for e in audit_requirements(docs)[1]))


if __name__ == "__main__":
    unittest.main()
