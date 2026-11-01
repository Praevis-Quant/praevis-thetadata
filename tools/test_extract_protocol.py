"""Descriptor recovery must refuse wheels that have not been researched; no vendor code runs."""
import tempfile
import unittest

import extract_protocol
from test_upstream_delta import OLD, wheel


class ExtractProtocolTests(unittest.TestCase):
    def test_unreviewed_version_is_refused_before_descriptor_parsing(self):
        with tempfile.TemporaryDirectory() as directory:
            path, _ = wheel(directory, "9.9.9", OLD)
            with self.assertRaisesRegex(ValueError, "9.9.9 has no reviewed artifact identity"):
                extract_protocol.recover(path)

    def test_reviewed_version_with_different_bytes_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            path, _ = wheel(directory, "1.0.12", OLD)
            with self.assertRaisesRegex(ValueError, "differs from the reviewed 1.0.12 artifact"):
                extract_protocol.recover(path)


if __name__ == "__main__":
    unittest.main()
