"""Credential order (ADR-0020): explicit arguments, then THETADATA_API_KEY,
a complete THETADATA_EMAIL / THETADATA_PASSWORD pair,
THETADATA_CREDENTIALS_FILE and ./creds.txt (no network)."""
import os
import pathlib
import tempfile
import unittest
from unittest import mock

import praevis_thetadata as pt

VARS = ("THETADATA_API_KEY", "THETADATA_EMAIL", "THETADATA_PASSWORD", "THETADATA_CREDENTIALS_FILE")


class Recorder:
    def __init__(self, **credentials_and_options):
        self.kwargs = credentials_and_options


class CredentialOrder(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.cwd = os.getcwd()
        os.chdir(self.directory.name)
        self.saved = {name: os.environ.pop(name, None) for name in VARS}
        patcher = mock.patch.object(pt._native, "Client", Recorder)
        patcher.start()
        self.addCleanup(patcher.stop)

    def tearDown(self):
        os.chdir(self.cwd)
        self.directory.cleanup()
        for name, value in self.saved.items():
            if value is not None:
                os.environ[name] = value
            else:
                os.environ.pop(name, None)

    def chosen(self, **arguments):
        return pt.Client(**arguments)._native.kwargs

    def test_the_vendor_order(self):
        pathlib.Path("creds.txt").write_text("default@example.invalid\nsynthetic\n")
        pathlib.Path("env.txt").write_text("env-file@example.invalid\nsynthetic\n")
        os.environ.update({
            "THETADATA_API_KEY": "synthetic-env-key",
            "THETADATA_EMAIL": "pair@example.invalid",
            "THETADATA_PASSWORD": "synthetic-password",
            "THETADATA_CREDENTIALS_FILE": "env.txt",
        })
        # Explicit arguments first: file, then key, then pair.
        self.assertEqual(self.chosen(creds_file="x.txt", api_key="k"), {"creds_file": "x.txt"})
        self.assertEqual(self.chosen(api_key="synthetic-key"), {"api_key": "synthetic-key"})
        self.assertEqual(self.chosen(email="a@example.invalid", password="p"),
                         {"email": "a@example.invalid", "password": "p"})
        # Then the environment, in order.
        self.assertEqual(self.chosen(), {"api_key": "synthetic-env-key"})
        os.environ["THETADATA_API_KEY"] = "   "  # blank falls through
        self.assertEqual(self.chosen(), {"email": "pair@example.invalid", "password": "synthetic-password"})
        del os.environ["THETADATA_PASSWORD"]  # incomplete pair falls through
        self.assertEqual(self.chosen(), {"creds_file": "env.txt"})
        del os.environ["THETADATA_CREDENTIALS_FILE"]
        self.assertEqual(self.chosen(), {"creds_file": "creds.txt"})
        self.assertEqual(pt.Client.from_env()._native.kwargs, {"creds_file": "creds.txt"})

    def test_missing_sources_explain_the_options_without_values(self):
        with self.assertRaises(pt.ThetaDataError) as caught:
            pt.Client()
        self.assertIn("THETADATA_API_KEY", str(caught.exception))
        os.environ["THETADATA_EMAIL"] = "pair@example.invalid"
        with self.assertRaises(pt.ThetaDataError) as caught:
            pt.Client()
        self.assertIn("only one of THETADATA_EMAIL and THETADATA_PASSWORD", str(caught.exception))
        self.assertNotIn("pair@example.invalid", str(caught.exception))
        with self.assertRaises(pt.ThetaDataError):
            pt.Client(email="a@example.invalid")  # a half pair is an error

    def test_options_reach_the_native_client(self):
        self.assertEqual(
            self.chosen(api_key="k", concurrency=3, per_minute=40),
            {"api_key": "k", "concurrency": 3, "per_minute": 40},
        )


if __name__ == "__main__":
    unittest.main()
