"""The live scripts parse, and refuse to run without the key or an id. They
are never run against the service here (docs/live-runbook.md)."""
import os
import re
import pathlib
import shutil
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
LIVE = ROOT / "scripts" / "live"
BASH = shutil.which("bash")


def run(*args, key=None):
    env = {k: v for k, v in os.environ.items() if k != "THETADATA_API_KEY"}
    if key:
        env["THETADATA_API_KEY"] = key
    return subprocess.run([BASH, str(LIVE / "run.sh"), *args], env=env, capture_output=True, text=True,
                          timeout=60)


@unittest.skipUnless(BASH, "bash is not available")
class LiveScriptTests(unittest.TestCase):
    def test_the_scripts_parse(self):
        for script in ("run.sh", "wsl.sh"):
            result = subprocess.run([BASH, "-n", str(LIVE / script)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_help_lists_the_options(self):
        result = run("--help")
        self.assertEqual(result.returncode, 0)
        for option in ("--run-id", "--part", "--terminal", "--full-trades-seconds", "--force-off-hours"):
            self.assertIn(option, result.stdout)

    def test_it_refuses_without_the_key_or_a_valid_run(self):
        self.assertEqual(run("--run-id", "x").returncode, 2)
        self.assertIn("THETADATA_API_KEY", run("--run-id", "x").stderr)
        self.assertEqual(run(key="not-used").returncode, 2)
        self.assertEqual(run("--run-id", "../x", key="not-used").returncode, 2)
        self.assertEqual(run("--run-id", "x", "--terminal", "restart", key="not-used").returncode, 2)

    def test_the_settings_template_holds_no_key_or_personal_path(self):
        text = (LIVE / "settings.example.env").read_text()
        self.assertNotIn("THETADATA_API_KEY=", text)
        # Only the placeholder user, never a real one.
        self.assertEqual(re.findall(r"Users\\([^\\\s]+)", text), ["<you>"])
        self.assertIn("scripts/live/settings.env", (ROOT / ".gitignore").read_text())


if __name__ == "__main__":
    unittest.main()
