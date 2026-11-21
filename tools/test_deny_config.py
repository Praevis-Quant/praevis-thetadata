"""deny.toml checks what we ship (offline; cargo-deny itself runs in CI)."""
import pathlib
import tomllib
import unittest

import release

DENY = tomllib.loads((pathlib.Path(__file__).resolve().parents[1] / "deny.toml").read_text(encoding="utf-8"))


class DenyConfig(unittest.TestCase):
    def test_every_released_platform_is_checked(self):
        self.assertEqual(sorted(DENY["graph"]["targets"]), sorted(release.CLI_TARGETS))
        self.assertTrue(DENY["graph"]["all-features"])

    def test_transitive_unmaintained_and_unsound_crates_fail(self):
        # cargo-deny's default scope ("workspace") checks only direct
        # dependencies, which let rustix 1.1.5 (RUSTSEC-2026-0486) through.
        self.assertEqual(DENY["advisories"]["unmaintained"], "all")
        self.assertEqual(DENY["advisories"]["unsound"], "all")


if __name__ == "__main__":
    unittest.main()
