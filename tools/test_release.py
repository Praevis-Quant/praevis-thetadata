"""Tests for tools/release.py (offline)."""
import pathlib
import tarfile
import tempfile
import unittest
import zipfile

import release


class GateTests(unittest.TestCase):
    def test_a_matching_tag_on_main_with_green_ci_passes(self):
        self.assertEqual(release.problems("push", "refs/tags/v0.1.3", "0.1.3", True, True, True), [])

    def test_a_tag_push_fails_on_each_missing_condition(self):
        found = release.problems("push", "refs/tags/v0.1.4", "0.1.3", False, None, False)
        self.assertEqual(len(found), 4, found)
        self.assertTrue(any("does not match" in f for f in found))
        self.assertTrue(any("not on main" in f for f in found))
        self.assertTrue(any("CI has not passed" in f for f in found))

    def test_a_dry_run_needs_only_the_draft(self):
        self.assertEqual(release.problems("workflow_dispatch", "refs/heads/main", "0.1.3", False, None, True), [])
        self.assertEqual(len(release.problems("pull_request", "refs/pull/1/merge", "0.1.3", False, None, False)), 1)

    def test_the_workspace_version_has_a_draft(self):
        self.assertTrue(release.draft(release.workspace_version()).is_file())


class ArtifactTests(unittest.TestCase):
    def test_archive_names(self):
        self.assertEqual(release.archive_name("0.1.3", "x86_64-pc-windows-msvc"),
                         "praevis-thetadata-cli-0.1.3-x86_64-pc-windows-msvc.zip")
        self.assertEqual(release.archive_name("0.1.3", "aarch64-unknown-linux-musl"),
                         "praevis-thetadata-cli-0.1.3-aarch64-unknown-linux-musl.tar.gz")
        self.assertEqual(len(release.expected_files("0.1.3")), 8)

    def test_packaged_archives_hold_the_binary_and_licences(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary = pathlib.Path(tmp, "bin")
            binary.write_bytes(b"\x7fELF")
            for target, opener in (("x86_64-unknown-linux-musl", "tar"), ("aarch64-pc-windows-msvc", "zip")):
                release.main(["package-cli", "--version", "0.1.3", "--target", target,
                              "--binary", str(binary), "--out", tmp])
                path = pathlib.Path(tmp, release.archive_name("0.1.3", target))
                if opener == "tar":
                    with tarfile.open(path) as archive:
                        members = {m.name: m for m in archive.getmembers()}
                    top = f"praevis-thetadata-cli-0.1.3-{target}"
                    self.assertEqual(members[f"{top}/praevis-thetadata"].mode, 0o755)
                    self.assertEqual(members[f"{top}/LICENSE"].mode, 0o644)
                else:
                    with zipfile.ZipFile(path) as archive:
                        names = set(archive.namelist())
                    self.assertIn(f"praevis-thetadata-cli-0.1.3-{target}/praevis-thetadata.exe", names)
                    self.assertIn(f"praevis-thetadata-cli-0.1.3-{target}/NOTICE", names)

    def test_sums_need_every_artifact(self):
        with tempfile.TemporaryDirectory() as tmp:
            for name in release.expected_files("0.1.3"):
                pathlib.Path(tmp, name).write_bytes(name.encode())
            with self.assertRaises(SystemExit):
                release.main(["sums", "--version", "0.1.3", "--dir", tmp])
            for i in range(release.WHEELS):
                pathlib.Path(tmp, f"praevis_thetadata-0.1.3-cp310-abi3-p{i}.whl").write_bytes(b"w")
            self.assertEqual(release.main(["sums", "--version", "0.1.3", "--dir", tmp]), 0)
            lines = pathlib.Path(tmp, "SHA256SUMS").read_text().splitlines()
            self.assertEqual(len(lines), 16)
            self.assertTrue(all(len(line.split("  ")[0]) == 64 for line in lines))


class PublishTests(unittest.TestCase):
    def test_the_waves_cover_the_seven_crates_in_dependency_order(self):
        order = [name for wave in sorted(release.WAVES) for name in release.WAVES[wave]]
        self.assertEqual(len(order), 7)
        self.assertLess(order.index("praevis-thetadata-proto"), order.index("praevis-thetadata-client"))
        self.assertLess(order.index("praevis-thetadata-client"), order.index("praevis-thetadata-cli"))

    def test_index_paths(self):
        self.assertEqual(release.index_path("praevis-thetadata-core"), "pr/ae/praevis-thetadata-core")


class NotesTests(unittest.TestCase):
    def test_the_body_follows_its_marker(self):
        self.assertEqual(release.release_body("# Draft\nmeta\n**Body:**\n\n> text\n"), "> text\n")
        with self.assertRaises(ValueError):
            release.release_body("no marker")

    def test_the_current_draft_has_a_body(self):
        body = release.release_body(release.draft(release.workspace_version()).read_text(encoding="utf-8"))
        self.assertIn("## Install", body)


if __name__ == "__main__":
    unittest.main()
