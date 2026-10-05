"""The rename must not strand benchmark provenance from older checkouts."""
from pathlib import Path
import tempfile
import unittest

from build_cli_benchmark import cli_identity


class CliIdentityTests(unittest.TestCase):
    def test_original_and_renamed_checkouts_select_their_own_package_and_binary(self):
        for package, binary in [("thetadata-cli", "theta"),
                                ("praevis-thetadata-cli", "praevis-thetadata")]:
            with self.subTest(package=package), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                manifest = root / "apps" / package / "Cargo.toml"
                manifest.parent.mkdir(parents=True)
                manifest.write_text(
                    f'[package]\nname = "{package}"\n[[bin]]\nname = "{binary}"\n',
                    encoding="utf-8")
                self.assertEqual(cli_identity(root), (package, binary))

    def test_missing_or_ambiguous_cli_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with self.assertRaises(ValueError):
                cli_identity(root)
            for name in ("thetadata-cli", "praevis-thetadata-cli"):
                manifest = root / "apps" / name / "Cargo.toml"
                manifest.parent.mkdir(parents=True)
                manifest.write_text("", encoding="utf-8")
            with self.assertRaises(ValueError):
                cli_identity(root)
