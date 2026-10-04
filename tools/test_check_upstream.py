import unittest
from unittest.mock import patch
import urllib.error

from check_upstream import check, fetch_index, main


def index(*versions):
    return {"name": "thetadata", "files": [
        {"filename": f"thetadata-{version}-py3-none-any.whl", "yanked": False}
        for version in versions
    ]}


class UpstreamTests(unittest.TestCase):
    def test_equal_older_and_newer_use_pep440_ordering(self):
        for version, expected in [("1.0.12", False), ("1.0.9", False), ("1.0.13", True),
                                  ("1.0.100", True), ("1.0.12.post1", True), ("1.0.12.0", False)]:
            with self.subTest(version=version):
                self.assertEqual(check(index(version), "1.0.12")[1], expected)

    def test_prerelease_dev_and_yanked_are_excluded(self):
        data = index("1.0.12", "2.0rc1", "2.0.dev1", "3.0", "4.0")
        data["files"][-2]["yanked"] = "withdrawn"
        data["files"][-1]["yanked"] = ""
        self.assertEqual(str(check(data, "1.0.12")[0]), "1.0.12")

    def test_source_distribution_and_unordered_versions(self):
        data = index("1.0.13", "1.0.9")
        data["files"].append({"filename": "thetadata-1.0.14.tar.gz"})
        self.assertEqual(str(check(data, "1.0.12")[0]), "1.0.14")

    def test_malformed_or_unusable_metadata_fails(self):
        for data in ({}, index(), index("2.0rc1"), {"name": "another", "files": []},
                     {"name": "thetadata", "files": [{"filename": "not-a-distribution"}]}):
            with self.subTest(data=data), self.assertRaises(ValueError):
                check(data, "1.0.12")

    def test_network_failure_retries_and_remains_a_failure(self):
        with patch("check_upstream.urllib.request.urlopen", side_effect=urllib.error.URLError("offline")) as request:
            with patch("check_upstream.time.sleep"), self.assertRaises(urllib.error.URLError):
                fetch_index()
            self.assertEqual(request.call_count, 3)

    def test_cli_exit_codes(self):
        with patch("check_upstream.fetch_index", return_value=index("999.0")):
            self.assertEqual(main(), 1)
        with patch("check_upstream.fetch_index", return_value=index("0.0.1")):
            self.assertEqual(main(), 0)
        with patch("check_upstream.fetch_index", side_effect=ValueError("invalid JSON")):
            self.assertEqual(main(), 2)


if __name__ == "__main__":
    unittest.main()
