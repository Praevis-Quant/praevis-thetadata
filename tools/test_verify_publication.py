"""Tests for tools/verify_publication.py (offline; the checks themselves run in CI)."""
import unittest

import release
import verify_publication as vp


def sums(version: str, extra: str = "") -> str:
    names = sorted(release.expected_files(version)) + [f"praevis_thetadata-{version}-cp310-abi3-p{i}.whl" for i in range(8)]
    return "".join(f"{i:064x}  {n}\n" for i, n in enumerate(names)) + extra


class Sums(unittest.TestCase):
    def test_a_complete_manifest_passes(self):
        self.assertEqual(len(vp.check_sums("0.1.5", sums("0.1.5"))), 31)

    def test_a_missing_extra_or_malformed_entry_fails(self):
        text = sums("0.1.5")
        for bad in (text.replace("praevis-thetadata-core-0.1.5.crate", "praevis-thetadata-core-0.1.4.crate"),
                    text + f"{0:064x}  unexpected.bin\n",
                    text.replace("  praevis-thetadata-proto", " praevis-thetadata-proto"),
                    text + text.splitlines(keepends=True)[0]):
            with self.assertRaises(vp.Failed):
                vp.check_sums("0.1.5", bad)

    def test_the_bundles_and_libraries(self):
        self.assertEqual(vp.bundle_names("0.1.5"), {"praevis-thetadata-0.1.5.intoto.jsonl",
                                                    "praevis-thetadata-cli-0.1.5.sbom.sigstore.jsonl",
                                                    "praevis-thetadata-wheel-0.1.5.sbom.sigstore.jsonl"})
        self.assertEqual(len(vp.LIBRARIES), 6)
        self.assertNotIn("praevis-thetadata-cli", vp.LIBRARIES)

    def test_no_credential_reaches_the_downloaded_executable(self):
        import os
        from unittest import mock
        with mock.patch.dict(os.environ, {"GH_TOKEN": "x", "GITHUB_TOKEN": "y", "PATH": "/bin"}):
            env = vp.public_env()
        self.assertNotIn("GH_TOKEN", env)
        self.assertNotIn("GITHUB_TOKEN", env)
        self.assertIn("PATH", env)


if __name__ == "__main__":
    unittest.main()
