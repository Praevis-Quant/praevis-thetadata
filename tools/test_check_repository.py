"""Mutations that must not pass the requirement hierarchy audit."""
import unittest

from check_repository import PACKAGE_DIRS, audit_anchors, audit_package_files, audit_requirements, heading_anchors


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



class PackageFileAuditTests(unittest.TestCase):
    def files(self):
        files = {"LICENSE": b"license\n", "NOTICE": b"notice\n",
                 "crates/praevis-thetadata-proto/LICENSE.upstream": b"upstream\n"}
        for directory in PACKAGE_DIRS:
            files.update({f"{directory}/LICENSE": b"license\r\n", f"{directory}/NOTICE": b"notice\n",
                          f"{directory}/README.md": b"# readme\n"})
        return files

    def audit(self, files):
        return audit_package_files(files.__getitem__, set(files))

    def test_identical_copies_pass_including_crlf_checkouts(self):
        self.assertEqual(self.audit(self.files()), [])

    def test_missing_or_drifted_copies_fail(self):
        files = self.files()
        del files["crates/praevis-thetadata-core/NOTICE"]
        files["apps/praevis-thetadata-cli/LICENSE"] = b"edited\n"
        del files["crates/praevis-thetadata-proto/LICENSE.upstream"]
        errors = self.audit(files)
        self.assertIn("Missing package file: crates/praevis-thetadata-core/NOTICE", errors)
        self.assertIn("Package copy differs from root LICENSE: apps/praevis-thetadata-cli/LICENSE", errors)
        self.assertIn("Missing upstream license in the proto package", errors)


if __name__ == "__main__":
    unittest.main()


class AnchorAuditTests(unittest.TestCase):
    TARGET = (
        "# Release `0.1.0-alpha.2` (2026-11-03)\n"
        "## Deferred from alpha.2\n"
        "## Status: alpha.2 **released**, next [alpha.3](x.md)\n"
        "## Repeat\n## Repeat\n"
        '<a id="explicit-id"></a>\n'
        "```text\n# not-a-heading\n```\n"
    )

    def test_github_slugs_duplicates_ids_and_fences(self):
        self.assertEqual(heading_anchors(self.TARGET), {
            "release-010-alpha2-2026-11-03", "deferred-from-alpha2",
            "status-alpha2-released-next-alpha3", "repeat", "repeat-1", "explicit-id"})

    def test_valid_relative_same_document_and_ignored_links(self):
        docs = {"docs/releases.md": self.TARGET + "[self](#repeat-1)",
                "docs/requirements/m.md": (
                    "[a](../releases.md#deferred-from-alpha2) [b](../releases.md#explicit-id) "
                    "[web](https://example.com/x.md#nope) [code](../../src/lib.rs#L10) "
                    "[missing file](../absent.md#nope) [plain](../releases.md)")}
        self.assertEqual(audit_anchors(docs), [])

    def test_broken_fragments_are_reported(self):
        for link in ("../releases.md#deferred-from-alpha3", "../releases.md#repeat-2",
                     "../releases.md#not-a-heading", "#nowhere"):
            with self.subTest(link=link):
                docs = {"docs/releases.md": self.TARGET, "docs/requirements/m.md": f"[x]({link})"}
                errors = audit_anchors(docs)
                self.assertEqual(len(errors), 1)
                self.assertIn(link, errors[0])
