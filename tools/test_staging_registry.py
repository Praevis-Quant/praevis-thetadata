"""Offline checks for the staging-registry rehearsal helpers; no cargo, network or registry."""
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

import staging_registry as staging


def crate(directory, name, manifest, extra=()):
    path = Path(directory) / f"{name}-0.1.0.crate"
    with tarfile.open(path, "w:gz") as tar:
        for member, data in [("Cargo.toml", manifest), ("LICENSE", "l"), ("NOTICE", "n"), ("README.md", "r"), *extra]:
            info = tarfile.TarInfo(f"{name}-0.1.0/{member}")
            info.size = len(data.encode())
            tar.addfile(info, io.BytesIO(data.encode()))
    return path


MANIFEST = """[package]
name = "praevis-thetadata-client"
version = "0.1.0"
rust-version = "1.88"

[dependencies.praevis-thetadata-auth]
version = "=0.1.0"

[dependencies.tokio]
version = "1"
features = ["macros"]

[build-dependencies.praevis-thetadata-proto]
version = "=0.1.0"

[target."cfg(windows)".dependencies.keyring]
version = "3.6.3"
optional = true

[features]
native-store = ["dep:keyring"]
"""


class StagingRegistryTests(unittest.TestCase):
    def test_index_paths_follow_the_cargo_layout(self):
        self.assertEqual(staging.index_path("ab"), Path("2/ab"))
        self.assertEqual(staging.index_path("abc"), Path("3/a/abc"))
        self.assertEqual(staging.index_path("Praevis-Thetadata-Auth"), Path("pr/ae/praevis-thetadata-auth"))

    def test_index_entry_keeps_ours_in_staging_and_third_party_on_crates_io(self):
        with tempfile.TemporaryDirectory() as directory:
            entry = staging.index_entry(crate(directory, "praevis-thetadata-client", MANIFEST))
        deps = {d["name"]: d for d in entry["deps"]}
        self.assertNotIn("registry", deps["praevis-thetadata-auth"])
        self.assertEqual(deps["praevis-thetadata-proto"]["kind"], "build")
        self.assertEqual(deps["tokio"]["registry"], staging.CRATES_IO_INDEX)
        self.assertEqual(deps["tokio"]["features"], ["macros"])
        self.assertEqual((deps["keyring"]["target"], deps["keyring"]["optional"]), ("cfg(windows)", True))
        self.assertEqual((entry["features"], entry["features2"]), ({}, {"native-store": ["dep:keyring"]}))
        self.assertEqual((entry["vers"], entry["rust_version"]), ("0.1.0", "1.88"))
        json.dumps(entry)

    def test_every_published_crate_is_staged_once_in_dependency_order(self):
        order = [name for wave in staging.WAVES for name in wave]
        self.assertEqual(len(order), len(set(order)))
        self.assertEqual(set(order), {
            "praevis-thetadata-auth", "praevis-thetadata-core", "praevis-thetadata-proto",
            "praevis-thetadata-arrow", "praevis-thetadata-client", "praevis-thetadata-cli",
            "praevis-thetadata-stream"})
        before = lambda a, b: order.index(a) < order.index(b)  # noqa: E731
        self.assertTrue(before("praevis-thetadata-arrow", "praevis-thetadata-client"))
        self.assertTrue(before("praevis-thetadata-client", "praevis-thetadata-stream"))
        self.assertNotIn("praevis-thetadata-py", order)

    def test_inspection_requires_license_files_and_rejects_secrets_and_artifacts(self):
        with tempfile.TemporaryDirectory() as directory:
            ok = staging.inspect(crate(directory, "praevis-thetadata-core", MANIFEST.replace("client", "core")))
            self.assertIn("NOTICE", ok["files"])
            for bad in [("creds.txt", "x"), (".env", "x"), ("artifacts/live/a.json", "x"), ("x.whl", "x"),
                        ("tests/cert.pem", "x"), ("server.key", "x"), ("id_ed25519", "x")]:
                with self.subTest(bad=bad[0]), self.assertRaises(SystemExit):
                    staging.inspect(crate(directory, "praevis-thetadata-core", MANIFEST, [bad]))
            with self.assertRaises(SystemExit):  # proto must carry the upstream license
                staging.inspect(crate(directory, "praevis-thetadata-proto", MANIFEST.replace("client", "proto")))


if __name__ == "__main__":
    unittest.main()
