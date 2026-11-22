"""Tests for tools/release.py (offline)."""
import pathlib
import tarfile
import tempfile
import unittest
import zipfile

import release


class GateTests(unittest.TestCase):
    OK = dict(on_main=True, ci_passed=True, notes_exist=True, dated=True, annotated=True, release_set=[])

    def test_a_matching_annotated_tag_on_main_with_green_ci_passes(self):
        self.assertEqual(release.problems("push", "refs/tags/v0.1.6", "0.1.6", **self.OK), [])

    def test_a_tag_push_fails_on_each_missing_condition(self):
        found = release.problems("push", "refs/tags/v0.1.7", "0.1.6", on_main=False, ci_passed=None, notes_exist=False,
                                 dated=False, annotated=False, release_set=["set differs"])
        self.assertEqual(len(found), 7, found)
        for text in ("does not match", "not an annotated tag", "not on main", "CI has not passed",
                     "no dated", "missing", "set differs"):
            self.assertTrue(any(text in f for f in found), text)

    def test_a_lightweight_or_unknown_tag_is_refused(self):
        for annotated in (False, None):
            found = release.problems("push", "refs/tags/v0.1.6", "0.1.6", **{**self.OK, "annotated": annotated})
            self.assertEqual(found, ["tag v0.1.6 is not an annotated tag"])

    def test_dry_runs_skip_the_tag_rules(self):
        self.assertEqual(release.problems("workflow_dispatch", "refs/heads/main", "0.1.6",
                                          **{**self.OK, "on_main": False, "ci_passed": None, "annotated": None}), [])

    def test_the_changelog_entry_must_be_dated(self):
        self.assertTrue(release.changelog_dated("0.1.6", "x\n## [0.1.6] - 2026-11-30\n"))
        self.assertFalse(release.changelog_dated("0.1.6", "## [0.1.6] - pending\n"))
        self.assertFalse(release.changelog_dated("0.1.6", "## [0.1.60] - 2026-11-30\n"))
        self.assertTrue(release.changelog_dated(
            release.workspace_version(), (release.ROOT / "CHANGELOG.md").read_text(encoding="utf-8")))

    def test_ci_counts_only_a_successful_push_to_main(self):
        run = {"event": "push", "head_branch": "main", "conclusion": "success"}
        self.assertTrue(release.ci_succeeded([{"conclusion": "failure"}, run]))
        for other in ({"event": "pull_request"}, {"head_branch": "v0.1.6"}, {"conclusion": None}):
            self.assertFalse(release.ci_succeeded([{**run, **other}]))

    def test_the_release_set_is_the_publishable_workspace(self):
        packages = [{"name": n, "version": "0.1.6", "publish": None} for n in release.CRATES]
        packages.append({"name": "praevis-thetadata-emulator", "version": "0.1.6", "publish": []})
        self.assertEqual(release.release_set_problems(packages, "0.1.6"), [])
        self.assertEqual(len(release.release_set_problems(packages[1:], "0.1.6")), 1)
        packages[0]["version"] = "0.1.5"
        self.assertEqual(len(release.release_set_problems(packages, "0.1.6")), 1)
        self.assertEqual(release.release_set_problems(release.workspace_packages(), release.workspace_version()), [])

    def test_the_workspace_version_has_notes(self):
        self.assertTrue(release.notes_file(release.workspace_version()).is_file())
        self.assertEqual(release.notes_file("0.1.6"), release.ROOT / "docs" / "releases" / "v0.1.6.md")
        self.assertEqual(release.notes_file("0.1.5"), release.ROOT / "docs" / "release-draft-0.1.5.md")


class LockfileTests(unittest.TestCase):
    def test_every_lockfile_names_our_crates_at_the_workspace_version(self):
        # A version bump must update every lockfile that names the workspace's
        # crates by path, or its locked build fails (release standard lesson:
        # fuzz/Cargo.lock).
        import subprocess
        import tomllib
        version = release.workspace_version()
        members = {p["name"] for p in release.workspace_packages()}
        locks = subprocess.run(["git", "ls-files", "*Cargo.lock", "**/Cargo.lock"], cwd=release.ROOT,
                               check=True, capture_output=True, text=True).stdout.split()
        self.assertGreaterEqual(len(locks), 3, locks)
        for lock in sorted(set(locks)):
            with open(release.ROOT / lock, "rb") as handle:
                packages = tomllib.load(handle)["package"]
            ours = [p for p in packages if p["name"] in members and "source" not in p]
            with self.subTest(lock):
                self.assertTrue(ours, lock)
                self.assertEqual({p["name"]: p["version"] for p in ours if p["version"] != version}, {})


class ArtifactTests(unittest.TestCase):
    def test_archive_names(self):
        self.assertEqual(release.archive_name("0.1.3", "x86_64-pc-windows-msvc"),
                         "praevis-thetadata-cli-0.1.3-x86_64-pc-windows-msvc.zip")
        self.assertEqual(release.archive_name("0.1.3", "aarch64-unknown-linux-musl"),
                         "praevis-thetadata-cli-0.1.3-aarch64-unknown-linux-musl.tar.gz")
        self.assertEqual(len(release.expected_files("0.1.3")), 23)
        self.assertIn("praevis-thetadata-client-0.1.3.crate", release.expected_files("0.1.3"))
        self.assertIn("praevis-thetadata-cli-0.1.3.cdx.json", release.expected_files("0.1.3"))

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
            self.assertEqual(len(lines), 31)
            self.assertTrue(all(len(line.split("  ")[0]) == 64 for line in lines))


class GitHubReleaseFlowTests(unittest.TestCase):
    """github_release end to end, against tools/fake_gh.py: every gh api call
    as the job makes it, on a fake that keeps drafts like GitHub and refuses
    any change to a published release."""

    def setUp(self):
        import json
        import os
        import sys
        from unittest import mock
        self.tmp = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.tmp.name)
        self.dist = root / "dist"
        self.dist.mkdir()
        for name, data in (("a.crate", b"crate"), ("b.whl", b"wheel"), ("SHA256SUMS", b"sums")):
            (self.dist / name).write_bytes(data)
        self.state = root / "state.json"
        self.state.write_text(json.dumps({"tags": ["v0.1.5"], "releases": [], "next_id": 0, "calls": []}))
        patches = [mock.patch.object(release, "GH", (sys.executable, str(release.ROOT / "tools" / "fake_gh.py"))),
                   mock.patch.dict(os.environ, {"FAKE_GH_STATE": str(self.state)})]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)
        self.addCleanup(self.tmp.cleanup)

    def run_release(self) -> int:
        return release.main(["github-release", "--version", "0.1.5", "--dir", str(self.dist), "--repo", "o/r"])

    def read(self) -> dict:
        import json
        return json.loads(self.state.read_text())

    def write(self, state: dict) -> None:
        import json
        self.state.write_text(json.dumps(state))

    def test_a_fresh_release_is_drafted_completed_checked_and_published(self):
        self.assertEqual(self.run_release(), 0)
        state = self.read()
        [published] = state["releases"]
        title, body = release.release_notes("0.1.5")
        self.assertEqual((published["draft"], published["tag_name"], published["name"], published["body"]),
                         (False, "v0.1.5", title, body))
        self.assertEqual(sorted(a["name"] for a in published["assets"]), ["SHA256SUMS", "a.crate", "b.whl"])
        methods = [method for method, _ in state["calls"]]
        self.assertEqual(methods, ["GET", "GET", "POST", "POST", "POST", "POST", "GET", "PATCH", "GET"])
        self.assertTrue(state["calls"][2][1].endswith("/releases"))  # created as a draft first
        self.assertTrue(all("uploads" not in path or "assets?name=" in path for _, path in state["calls"]))

    def test_a_re_run_after_publishing_changes_nothing(self):
        self.assertEqual(self.run_release(), 0)
        state = self.read()
        state["calls"] = []
        self.write(state)
        self.assertEqual(self.run_release(), 0)
        self.assertEqual({method for method, _ in self.read()["calls"]}, {"GET"})

    def test_a_missing_tag_stops_before_anything_is_created(self):
        state = self.read()
        state["tags"] = []
        self.write(state)
        self.assertEqual(self.run_release(), 1)
        self.assertEqual(self.read()["releases"], [])

    def test_a_draft_with_an_interrupted_upload_is_completed(self):
        title, body = release.release_notes("0.1.5")
        state = self.read()
        state["next_id"] = 10
        state["releases"] = [{"id": 1, "tag_name": "v0.1.5", "name": title, "body": body, "draft": True,
                              "prerelease": False,
                              "assets": [{"id": 9, "name": "a.crate", "size": 2, "state": "starter",
                                          "digest": None}]}]
        self.write(state)
        self.assertEqual(self.run_release(), 0)
        state = self.read()
        [published] = state["releases"]
        self.assertFalse(published["draft"])
        self.assertNotIn(9, [a["id"] for a in published["assets"]])
        self.assertIn(["DELETE", "repos/o/r/releases/assets/9"], state["calls"])

    def test_a_published_release_that_differs_is_reported_never_changed(self):
        self.assertEqual(self.run_release(), 0)
        (self.dist / "a.crate").write_bytes(b"different")
        state = self.read()
        state["calls"] = []
        self.write(state)
        self.assertEqual(self.run_release(), 1)
        self.assertEqual({method for method, _ in self.read()["calls"]}, {"GET"})
class SbomTests(unittest.TestCase):
    def test_an_sbom_keeps_only_what_the_package_builds(self):
        def component(name, version):
            return {"name": name, "version": version, "bom-ref": f"registry#{name}@{version}"}
        document = {
            "metadata": {"component": {"name": "pkg", "bom-ref": "path#pkg"}},
            "components": [component("a", "1.0.0"), component("openssl", "0.10.0"), component("a", "2.0.0")],
            "dependencies": [{"ref": "path#pkg", "dependsOn": ["registry#a@1.0.0", "registry#openssl@0.10.0"]},
                             {"ref": "registry#openssl@0.10.0", "dependsOn": []}],
        }
        removed = release.limit_sbom(document, {("a", "1.0.0")})
        self.assertEqual(removed, ["a@2.0.0", "openssl@0.10.0"])
        self.assertEqual([c["name"] for c in document["components"]], ["a"])
        self.assertEqual(document["dependencies"], [{"ref": "path#pkg", "dependsOn": ["registry#a@1.0.0"]}])
        self.assertIn({"name": "praevis-thetadata:sbom:graph", "value": release.GRAPH_METHOD},
                      document["metadata"]["properties"])

    def test_the_build_graph_follows_each_package_s_own_features(self):
        # The workspace's unified features put the CLI's native store under
        # auth; auth's own build has no keyring, and nothing builds OpenSSL.
        auth = {name for name, _ in release.build_graph("praevis-thetadata-auth")}
        cli = {name for name, _ in release.build_graph("praevis-thetadata-cli")}
        self.assertNotIn("keyring", auth)
        self.assertIn("keyring", cli)
        self.assertFalse({"openssl", "openssl-sys"} & (auth | cli))


class PublishTests(unittest.TestCase):
    def test_the_waves_cover_the_seven_crates_in_dependency_order(self):
        order = [name for wave in sorted(release.WAVES) for name in release.WAVES[wave]]
        self.assertEqual(len(order), 7)
        self.assertLess(order.index("praevis-thetadata-proto"), order.index("praevis-thetadata-client"))
        self.assertLess(order.index("praevis-thetadata-client"), order.index("praevis-thetadata-cli"))

    def test_the_crates_are_in_publish_order(self):
        self.assertEqual(release.CRATES[0], "praevis-thetadata-proto")
        self.assertEqual(set(release.CRATES), {n for w in release.WAVES.values() for n in w})

    def test_a_downloaded_crate_must_equal_the_attested_one(self):
        import hashlib
        data = b"crate bytes"
        good = hashlib.sha256(data).hexdigest()
        name, version = "praevis-thetadata-core", "0.1.5"
        self.assertIsNone(release.download_mismatch(name, version, good, data, good))
        self.assertIn("crates.io serves", release.download_mismatch(name, version, good, b"other", good))
        self.assertIn("index lists", release.download_mismatch(name, version, good, data, "0" * 64))
        self.assertIn("not downloadable", release.download_mismatch(name, version, good, None, None))

    def test_fetch_retries_only_transient_failures(self):
        import io
        import urllib.error
        from unittest import mock
        calls = []

        def opener(errors):
            def urlopen(request, timeout):
                calls.append(1)
                if errors:
                    raise errors.pop(0)
                return io.BytesIO(b"ok")
            return urlopen
        with mock.patch("time.sleep"), mock.patch("urllib.request.urlopen",
                                                   opener([urllib.error.URLError("dns"), urllib.error.HTTPError("u", 503, "x", {}, None)])):
            self.assertEqual(release.fetch("https://example.invalid", delays=(0, 0, 0)), b"ok")
        self.assertEqual(len(calls), 3)
        with mock.patch("time.sleep"), mock.patch("urllib.request.urlopen",
                                                   opener([urllib.error.HTTPError("u", 404, "x", {}, None)])):
            with self.assertRaises(urllib.error.HTTPError):
                release.fetch("https://example.invalid", delays=(0, 0))
        with mock.patch("time.sleep"), mock.patch("urllib.request.urlopen",
                                                   opener([urllib.error.URLError("dns")] * 3)):
            with self.assertRaises(urllib.error.URLError):
                release.fetch("https://example.invalid", delays=(0, 0))

    def test_the_known_good_version_covers_every_crate(self):
        self.assertEqual(set(release.KNOWN_GOOD[1]), set(release.CRATES))

    def test_crate_mismatches_name_each_difference(self):
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
            for name in release.CRATES:
                pathlib.Path(a, release.crate_file(name, "0.1.5")).write_bytes(name.encode())
                pathlib.Path(b, release.crate_file(name, "0.1.5")).write_bytes(name.encode())
            self.assertEqual(release.crate_mismatches("0.1.5", pathlib.Path(a), pathlib.Path(b)), [])
            pathlib.Path(b, release.crate_file("praevis-thetadata-cli", "0.1.5")).write_bytes(b"changed")
            pathlib.Path(b, release.crate_file("praevis-thetadata-core", "0.1.5")).unlink()
            found = release.crate_mismatches("0.1.5", pathlib.Path(a), pathlib.Path(b))
            self.assertEqual(len(found), 2, found)

    def test_index_paths(self):
        self.assertEqual(release.index_path("praevis-thetadata-core"), "pr/ae/praevis-thetadata-core")


class NotesTests(unittest.TestCase):
    def test_the_body_follows_its_marker(self):
        self.assertEqual(release.release_body("# Draft\nmeta\n**Body:**\n\n> text\n"), "> text\n")
        with self.assertRaises(ValueError):
            release.release_body("no marker")

    def test_the_provenance_bundle_has_the_suffix_scorecard_recognises(self):
        # OpenSSF Scorecard's Signed-Releases check counts a release asset
        # ending in .intoto.jsonl as provenance (probes/releasesHaveProvenance).
        workflow = (release.ROOT / ".github" / "workflows" / "build-release.yml").read_text(encoding="utf-8")
        self.assertIn('"dist/praevis-thetadata-$VERSION.intoto.jsonl"', workflow)
        self.assertNotIn("provenance.sigstore.jsonl", workflow)
        version = release.workspace_version()
        body = release.release_notes(version)[1]
        self.assertIn(f"`praevis-thetadata-{version}.intoto.jsonl`", body)

    def test_every_documented_verification_names_the_release_tag(self):
        # Dry runs attest too, from a branch; only --source-ref tells the
        # release's attestation apart for a reproducible .crate.
        docs = [release.ROOT / "docs" / "releasing.md", release.ROOT / "docs" / "security" / "assurance-report.md",
                release.notes_file(release.workspace_version())]
        for doc in docs:
            commands = doc.read_text(encoding="utf-8").split("gh attestation verify ")[1:]
            self.assertTrue(commands, doc)
            for command in commands:
                text = command.split("```")[0].split("\n\n")[0]
                if text.startswith("--bundle` checks"):
                    continue  # prose about the command, not a command
                with self.subTest(doc=doc.name, command=text[:50]):
                    self.assertIn("--source-ref refs/tags/v", text)

    def test_the_current_notes_have_a_body(self):
        title, body = release.release_notes(release.workspace_version())
        self.assertIn("## Install", body)
        self.assertTrue(title)

    def test_new_notes_are_the_whole_file_titled_by_its_heading(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "docs" / "releases").mkdir(parents=True)
            notes = root / "docs" / "releases" / "v0.1.6.md"
            notes.write_text("# praevis-thetadata 0.1.6\n\nText.\n")
            self.assertEqual(release.release_notes("0.1.6", root),
                             ("praevis-thetadata 0.1.6", "# praevis-thetadata 0.1.6\n\nText.\n"))
            notes.write_text("no heading\n")
            with self.assertRaises(ValueError):
                release.release_notes("0.1.6", root)


class GitHubReleaseTests(unittest.TestCase):
    FILES = {"a.crate": (3, "a" * 64), "SHA256SUMS": (10, "b" * 64)}

    def release(self, **extra):
        assets = [{"name": n, "size": s, "digest": f"sha256:{d}", "state": "uploaded"}
                  for n, (s, d) in self.FILES.items()]
        return {"tag_name": "v0.1.6", "name": "T", "body": "Body", "draft": False, "assets": assets, **extra}

    def test_a_matching_published_release_passes(self):
        self.assertEqual(release.release_problems(self.release(), "v0.1.6", "T", "Body\n", self.FILES,
                                                  published=True), [])

    def test_each_difference_is_reported(self):
        bad = self.release(name="X", body="Other", draft=True)
        bad["assets"][0]["digest"] = "sha256:" + "c" * 64
        bad["assets"].append({"name": "extra", "size": 1, "digest": "sha256:" + "d" * 64, "state": "uploaded"})
        found = release.release_problems(bad, "v0.1.6", "T", "Body\n", self.FILES, published=True)
        self.assertEqual(len(found), 5, found)

    def test_a_draft_uploads_only_missing_files_and_never_replaces_one(self):
        draft = self.release(draft=True)
        draft["assets"] = draft["assets"][:1]
        draft["assets"][0]["id"] = 7
        self.assertEqual(release.uploads_needed(draft, self.FILES), (["SHA256SUMS"], [], []))
        draft["assets"][0]["state"] = "starter"  # an interrupted upload
        self.assertEqual(release.uploads_needed(draft, self.FILES), (["SHA256SUMS", "a.crate"], [7], []))
        draft["assets"][0]["state"] = "uploaded"
        draft["assets"][0]["size"] = 4
        self.assertEqual(release.uploads_needed(draft, self.FILES), (["SHA256SUMS"], [], ["a.crate"]))


if __name__ == "__main__":
    unittest.main()
