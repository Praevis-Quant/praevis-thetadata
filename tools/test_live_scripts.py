"""The live scripts parse, and refuse to run without the key or an id. They
are never run against the service here (docs/live-runbook.md)."""
import os
import re
import pathlib
import shutil
import subprocess
import tempfile
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


def load(text):
    """Loads ``text`` as a settings file; returns (exit code, values, stderr)."""
    with tempfile.TemporaryDirectory() as directory:
        path = pathlib.Path(directory) / "settings.env"
        path.write_bytes(text.encode())
        marker = pathlib.Path(directory) / "executed"
        script = (f'. "{(LIVE / "settings.sh").as_posix()}"; load_settings "$1" || exit $?; '
                  'for k in THETA_JAVA THETA_TERMINAL_DIR THETA_REST WSL_REPO; do printf "%s=%s\\n" "$k" "${!k-<unset>}"; done')
        result = subprocess.run([BASH, "-c", script, "bash", path.as_posix()], capture_output=True, text=True,
                                env={**os.environ, "HOME": "/home/emulated"}, cwd=directory, timeout=30)
        values = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
        return result.returncode, values, result.stderr, marker.exists()


@unittest.skipUnless(BASH, "bash is not available")
class SettingsTests(unittest.TestCase):
    def test_a_path_with_spaces_and_backslashes_is_taken_literally(self):
        java = r"C:\Program Files\Eclipse Adoptium\jdk-21\bin\javaw.exe"
        code, values, err, _ = load(f"# comment\n\nTHETA_JAVA={java}\r\nTHETA_TERMINAL_DIR='C:\\Users\\<you>\\.thetadata'\n")
        self.assertEqual(code, 0, err)
        self.assertEqual(values["THETA_JAVA"], java)
        self.assertEqual(values["THETA_TERMINAL_DIR"], r"C:\Users\<you>\.thetadata")

    def test_quotes_and_the_home_folder(self):
        code, values, err, _ = load('THETA_REST="127.0.0.1:25503"\nWSL_REPO=~/praevis-thetadata\n'
                                    'THETA_TERMINAL_DIR=$HOME/thetadata\n')
        self.assertEqual(code, 0, err)
        self.assertEqual(values["THETA_REST"], "127.0.0.1:25503")
        self.assertEqual(values["WSL_REPO"], "/home/emulated/praevis-thetadata")
        self.assertEqual(values["THETA_TERMINAL_DIR"], "/home/emulated/thetadata")

    def test_nothing_in_the_file_is_executed(self):
        code, values, err, executed = load("THETA_JAVA=$(touch executed)\nTHETA_REST=`touch executed`\n")
        self.assertEqual(code, 0, err)
        self.assertFalse(executed)
        self.assertEqual(values["THETA_JAVA"], "$(touch executed)")

    def test_a_bad_line_or_an_unknown_setting_stops_with_its_line(self):
        code, _, err, executed = load("THETA_REST=127.0.0.1:25503\ntouch executed\n")
        self.assertEqual(code, 1)
        self.assertIn(":2: not KEY=VALUE", err)
        self.assertFalse(executed)
        code, _, err, _ = load("PATH=/tmp\n")
        self.assertEqual(code, 1)
        self.assertIn("unknown setting PATH", err)


if __name__ == "__main__":
    unittest.main()
